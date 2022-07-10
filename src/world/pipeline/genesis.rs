use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
    utils::{HashMap, HashSet},
};
use bracket_noise::prelude::{FastNoise, FractalType, NoiseType};
use futures_lite::future;
use serde::{Deserialize, Serialize};

use crate::world::{
    math,
    storage::{
        chunk::{self, ChunkKind},
        voxel, VoxWorld,
    },
};

const CACHE_PATH: &str = "cache/chunks/";
const CACHE_EXT: &str = "bin";

pub(super) struct GenesisPlugin;

impl Plugin for GenesisPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkLoaded>()
            .add_event::<EvtChunkUnloaded>()
            .add_event::<EvtChunkUpdated>()
            .add_startup_system_to_stage(super::PipelineStartup::Genesis, setup_resources)
            .add_system_to_stage(
                super::Pipeline::Genesis,
                update_world_system.label("update"),
            );
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ChunkCache {
    local: IVec3,
    kind: ChunkKind,
}

#[cfg(test)]
impl PartialEq for ChunkCache {
    fn eq(&self, other: &Self) -> bool {
        self.local == other.local && self.kind == other.kind
    }
}

pub struct EvtChunkLoaded(pub IVec3);
pub struct EvtChunkUnloaded(pub IVec3);
pub struct EvtChunkUpdated(pub IVec3);

fn setup_resources(mut commands: Commands) {
    trace_system_run!();

    if !std::path::Path::new(CACHE_PATH).exists() {
        std::fs::create_dir_all(CACHE_PATH).unwrap();
    }

    commands.insert_resource(WorldRes(Some(VoxWorld::default())));
    commands.insert_resource(BatchChunkCmdRes::default());
}

/**
Hold chunk commands to be processed in batch.
Internally uses a double buffered list of commands to keep track of what is running and what is pending.
*/
#[derive(Default)]
pub struct BatchChunkCmdRes {
    pending: Vec<ChunkCmd>,
    running: Vec<ChunkCmd>,
}

impl BatchChunkCmdRes {
    /**
    Swap the running and pending buffers

    Returns a clone of the running buffer
     */
    fn swap_and_clone(&mut self) -> Vec<ChunkCmd> {
        // Since the running buffer is always cleared when the batch is finished, this swap has no side-effects
        std::mem::swap(&mut self.running, &mut self.pending);

        debug!("Running: {:?}", Self::count_chunk_cmd(&self.running));

        self.running.clone()
    }

    /**
    Clears the running buffer
    */
    fn finished(&mut self) {
        debug!("Finished!");
        self.running.clear();
    }

    /**
    Checks if there is pending commands to be processed
     */
    pub fn has_pending_commands(&self) -> bool {
        self.pending.is_empty()
    }

    /**
    Adds a load command to the batch
     */
    pub fn load(&mut self, local: IVec3) {
        self.pending.push(ChunkCmd::Load(local));
    }

    /**
    Adds an unload command to the batch
     */
    pub fn unload(&mut self, local: IVec3) {
        self.pending.push(ChunkCmd::Unload(local));
    }

    /**
    Adds an update command to the batch
     */
    pub fn update(&mut self, local: IVec3, voxels: Vec<(IVec3, voxel::Kind)>) {
        self.pending.push(ChunkCmd::Update(local, voxels));
    }

    fn count_chunk_cmd(vec: &Vec<ChunkCmd>) -> (i32, i32, i32) {
        vec.iter()
            .map(|c| match &c {
                ChunkCmd::Load(_) => (1, 0, 0),
                ChunkCmd::Unload(_) => (0, 1, 0),
                ChunkCmd::Update(_, _) => (0, 0, 1),
            })
            .fold((0, 0, 0), |s, v| (s.0 + v.0, s.1 + v.1, s.2 + v.2))
    }
}

impl std::fmt::Display for BatchChunkCmdRes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (pending_load, pending_unload, pending_update) = Self::count_chunk_cmd(&self.pending);
        let (running_load, running_unload, running_update) = Self::count_chunk_cmd(&self.running);

        write!(
            f,
            "Running LD: {} UL: {} UP: {} | Pending LD: {} UL: {} UP: {}",
            running_load,
            running_unload,
            running_update,
            pending_load,
            pending_unload,
            pending_update,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ChunkCmd {
    Load(IVec3),
    Unload(IVec3),
    Update(IVec3, Vec<(IVec3, voxel::Kind)>),
}

#[derive(Clone, Debug, PartialEq)]
enum ChunkCmdResult {
    Loaded(IVec3),
    Unloaded(IVec3),
    Updated(IVec3),
}

pub struct WorldRes(Option<VoxWorld>);

impl WorldRes {
    pub fn is_ready(&self) -> bool {
        self.0.is_some()
    }

    pub fn take(&mut self) -> VoxWorld {
        self.0
            .take()
            .expect("You can take world only when it's ready")
    }

    pub fn set(&mut self, world: VoxWorld) {
        assert!(
            self.0.replace(world).is_none(),
            "There can be only one world at a time"
        );
    }
}

impl Deref for WorldRes {
    type Target = VoxWorld;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("WorldRes should be ready")
    }
}

/**
 * This is a meta data struct used only by [`update_world_system`]
 */
#[derive(Default)]
struct ProcessBatchSystemMeta {
    running_task: Option<Task<(VoxWorld, Vec<ChunkCmdResult>)>>,
}

/**
 * System which process pending commands and updates the world.
 *
 * Since there can be only one copy of World at any given time, when this system process commands
 * it takes ownership of the [`VoxWorld`] from [`WorldRes`] until all the batch is processed.
 *
 * This can take several frames.
 */
fn update_world_system(
    task_pool: Res<AsyncComputeTaskPool>,
    mut batch_res: ResMut<BatchChunkCmdRes>,
    mut meta: Local<ProcessBatchSystemMeta>,
    mut world_res: ResMut<WorldRes>,
    mut loaded_writer: EventWriter<EvtChunkLoaded>,
    mut unloaded_writer: EventWriter<EvtChunkUnloaded>,
    mut updated_writer: EventWriter<EvtChunkUpdated>,
) {
    let mut _perf = perf_fn!();

    if let Some(ref mut task) = meta.running_task {
        perf_scope!(_perf);

        // Check if task has finished
        if let Some((world, commands)) = future::block_on(future::poll_once(task)) {
            // Dispatch all events generated from this batch
            for cmd in commands {
                match cmd {
                    ChunkCmdResult::Loaded(local) => loaded_writer.send(EvtChunkLoaded(local)),
                    ChunkCmdResult::Unloaded(local) => {
                        unloaded_writer.send(EvtChunkUnloaded(local))
                    }
                    ChunkCmdResult::Updated(local) => updated_writer.send(EvtChunkUpdated(local)),
                }
            }

            // Give back the VoxWorld to WorldRes
            meta.running_task = None;
            world_res.set(world);
            batch_res.finished();
        }
    } else if !batch_res.has_pending_commands() {
        perf_scope!(_perf);
        let batch = batch_res.swap_and_clone();
        let world = world_res.take();

        meta.running_task = Some(task_pool.spawn(async move { process_batch(world, batch) }));
    }

    assert_ne!(
        meta.running_task.is_none(),
        !world_res.is_ready(),
        "The world should exists only in one place at any given time"
    );
    assert_ne!(
        meta.running_task.is_some(),
        world_res.is_ready(),
        "The world should exists only in one place at any given time"
    );
}

/**
This functions optimize the command list removing duplicated commands or commands that nullifies each other.

**Rules**
 1. Skips any duplicated commands (*Load* -> *Load*, *Update* -> *Update*, *Unload* -> *Unload*).
 2. Skips *Load* and remove existing *Unload* cmd when chunk exists already.
 3. Skips *Unload* and remove existing *Load* cmd when chunk doesn't exists already.
 4. Skips *Unload* when chunk doesn't exists already.
 5. Skips *Load* when chunk exists already.
 6. Skips *Update* if the chunk doesn't exists already.
 7. Replaces *Update* by *Unload* if the chunk exists already.
 8. Replaces *Update* by *Load* if the chunk doesn't exists already. [Removed]
 9. Skips *Update* if there is an *Unload* cmd already.

**This functions does preserves the insertion order**

**Returns** an optmized command list
*/
fn optimize_commands(world: &VoxWorld, commands: Vec<ChunkCmd>) -> Vec<ChunkCmd> {
    let mut map = HashMap::<IVec3, (u32, ChunkCmd)>::new();

    // Used to preserve command insertion order
    let mut order = 0u32;

    for cmd in commands {
        match cmd {
            ChunkCmd::Load(local) => {
                let chunk_exists = world.get(local).is_some();

                if let Some((_, existing_cmd)) = map.get(&local) {
                    match existing_cmd {
                        ChunkCmd::Load(_) => continue, // Rule 1
                        ChunkCmd::Unload(_) if chunk_exists => {
                            // Rule 2
                            map.remove(&local);
                            continue;
                        }
                        _ => {
                            panic!(
                                "Undefined behaviour for {:?} and {:?} when chunk_exists = {:?}",
                                cmd, existing_cmd, chunk_exists
                            );
                        }
                    }
                } else if chunk_exists {
                    // Rule 5
                    continue;
                }

                order += 1;
                let existing = map.insert(local, (order, cmd));

                debug_assert!(existing.is_none(), "This should never happens, since all existing cases should be handled by above match");
            }
            ChunkCmd::Unload(local) => {
                let chunk_exists = world.get(local).is_some();

                if let Some((_, existing_cmd)) = map.get(&local) {
                    match existing_cmd {
                        ChunkCmd::Unload(_) => continue, // Rule 1
                        ChunkCmd::Load(_) if !chunk_exists => {
                            // Rule 3
                            map.remove(&local);
                            continue;
                        }
                        ChunkCmd::Update(_, _) if chunk_exists => {
                            // Rule 7
                            order += 1;
                            map.insert(local, (order, cmd.clone()));
                            continue;
                        }
                        _ => {
                            panic!(
                                "Undefined behaviour for {:?} and {:?} when chunk_exists = {:?}",
                                cmd, existing_cmd, chunk_exists
                            );
                        }
                    }
                } else if !chunk_exists {
                    // Rule 4
                    continue;
                }

                order += 1;
                let existing = map.insert(local, (order, cmd));

                debug_assert!(existing.is_none(), "This should never happens, since all existing cases should be handled by above match");
            }
            ChunkCmd::Update(local, _) => {
                if world.get(local).is_none() {
                    // Rule 6
                    continue;
                }

                if let Some((_, existing_cmd)) = map.get(&local) {
                    match existing_cmd {
                        ChunkCmd::Update(_, _) => continue, // Rule 1. TODO: Maybe merge update data in the future?
                        ChunkCmd::Unload(_) => continue,    // Rule 9.
                        _ => {
                            panic!("Undefined behaviour for {:?} and {:?}", cmd, existing_cmd);
                        }
                    }
                }

                order += 1;
                let existing = map.insert(local, (order, cmd));

                debug_assert!(existing.is_none(), "This should never happens, since all existing cases should be handled by above match");
            }
        }
    }

    // TODO: Change this to `into_values` when Bevy is updated to 0.8
    let mut values = map.values().collect::<Vec<_>>();
    values.sort_by(|&t1, &t2| t1.0.cmp(&t2.0));

    values.into_iter().map(|(_, cmd)| cmd.clone()).collect()
}

fn process_batch(mut world: VoxWorld, commands: Vec<ChunkCmd>) -> (VoxWorld, Vec<ChunkCmdResult>) {
    let mut _perf = perf_fn!();

    let commands = optimize_commands(&world, commands);

    let unload_items = commands
        .iter()
        .filter_map(|cmd| match cmd {
            ChunkCmd::Unload(local) => Some(*local),
            _ => None,
        })
        .collect::<Vec<_>>();

    let load_items = commands
        .iter()
        .filter_map(|cmd| match cmd {
            ChunkCmd::Load(local) => Some(*local),
            _ => None,
        })
        .collect::<Vec<_>>();

    let update_items = commands
        .iter()
        .filter_map(|cmd| match cmd {
            ChunkCmd::Update(local, ref v) => Some((*local, v)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();

    let mut dirty_chunks = HashSet::default();

    for local in unload_items.iter() {
        perf_scope!(_perf);

        dirty_chunks.extend(unload_chunk(&mut world, *local));
    }

    for local in load_items.iter() {
        perf_scope!(_perf);
        dirty_chunks.extend(load_chunk(&mut world, *local));
    }

    for (local, voxels) in update_items.iter() {
        perf_scope!(_perf);
        dirty_chunks.extend(update_voxel(&mut world, *local, voxels));
    }

    let mut result = dirty_chunks
        .drain()
        .filter(|local| update_chunk(&mut world, *local))
        .map(ChunkCmdResult::Updated)
        .collect::<Vec<_>>();

    result.extend(unload_items.into_iter().map(ChunkCmdResult::Unloaded));
    result.extend(load_items.into_iter().map(ChunkCmdResult::Loaded));

    (world, result)
}

fn update_voxel(
    world: &mut VoxWorld,
    local: IVec3,
    voxels: &[(IVec3, voxel::Kind)],
) -> HashSet<IVec3> {
    trace!("Updating chunk {} values {:?}", local, voxels);
    let mut dirty_chunks = HashSet::default();

    if let Some(chunk) = world.get_mut(local) {
        for (voxel, kind) in voxels {
            chunk.set(*voxel, *kind);

            if chunk::is_at_bounds(*voxel) {
                let neighbor_dir = chunk::get_boundary_dir(*voxel);
                for unit_dir in math::to_unit_dir(neighbor_dir) {
                    let neighbor = unit_dir + local;
                    dirty_chunks.insert(neighbor);
                }
            }
        }

        dirty_chunks.insert(local);
    } else {
        warn!("Failed to set voxel. Chunk {} wasn't found.", local);
    }

    dirty_chunks
}

fn unload_chunk(world: &mut VoxWorld, local: IVec3) -> HashSet<IVec3> {
    perf_fn_scope!();

    let mut dirty_chunks = HashSet::default();

    if world.remove(local).is_none() {
        warn!("Trying to unload non-existing chunk {}", local);
    } else {
        dirty_chunks.extend(voxel::SIDES.map(|s| s.dir() + local))
    }

    dirty_chunks
}

fn load_chunk(world: &mut VoxWorld, local: IVec3) -> HashSet<IVec3> {
    perf_fn_scope!();

    let path = local_path(local);

    let cache = if path.exists() {
        load_cache(&path)
    } else {
        generate_cache(local)
    };

    world.add(local, cache.kind);

    voxel::SIDES
        .iter()
        .map(|s| s.dir() + local)
        .chain(std::iter::once(local))
        .collect()
}

fn update_chunk(world: &mut VoxWorld, local: IVec3) -> bool {
    perf_fn_scope!();

    if world.get(local).is_some() {
        world.update_neighborhood(local);
        true
    } else {
        false
    }
}

fn generate_cache(local: IVec3) -> ChunkCache {
    perf_fn_scope!();

    let mut noise = FastNoise::seeded(15);
    noise.set_noise_type(NoiseType::SimplexFractal);
    noise.set_frequency(0.03);
    noise.set_fractal_type(FractalType::FBM);
    noise.set_fractal_octaves(3);
    noise.set_fractal_gain(0.9);
    noise.set_fractal_lacunarity(0.5);
    let world = chunk::to_world(local);
    let mut kinds = ChunkKind::default();
    for x in 0..chunk::X_AXIS_SIZE {
        for z in 0..chunk::Z_AXIS_SIZE {
            let h = noise.get_noise(world.x + x as f32, world.z + z as f32);
            let world_height = ((h + 1.0) / 2.0) * (chunk::X_AXIS_SIZE * 2) as f32;

            let height_local = world_height - world.y;

            if height_local < f32::EPSILON {
                continue;
            }

            let end = usize::min(height_local as usize, chunk::Y_AXIS_SIZE);

            for y in 0..end {
                kinds.set((x as i32, y as i32, z as i32).into(), 1.into());
            }
        }
    }
    let path = local_path(local);

    assert!(!path.exists(), "Cache already exists!");

    let chunk_cache = ChunkCache { local, kind: kinds };
    save_cache(&path, &chunk_cache);

    chunk_cache
}

fn save_cache(path: &Path, cache: &ChunkCache) {
    perf_fn_scope!();

    let file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to write to file {}", path.display()));

    #[cfg(not(feature = "serde_ron"))]
    bincode::serialize_into(file, cache)
        .unwrap_or_else(|_| panic!("Failed to serialize cache to file {}", path.display()));

    #[cfg(feature = "serde_ron")]
    ron::ser::to_writer(file, cache)
        .unwrap_or_else(|_| panic!("Failed to serialize cache to file {}", path.display()));
}

fn load_cache(path: &Path) -> ChunkCache {
    perf_fn_scope!();

    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to open file {}", path.display()));

    #[cfg(not(feature = "serde_ron"))]
    let cache = bincode::deserialize_from(file)
        .unwrap_or_else(|_| panic!("Failed to parse file {}", path.display()));

    #[cfg(feature = "serde_ron")]
    let cache =
        ron::de::from_reader(file).expect(&format!("Failed to parse file {}", path.display()));

    cache
}

fn local_path(local: IVec3) -> PathBuf {
    PathBuf::from(CACHE_PATH)
        .with_file_name(format_local(local))
        .with_extension(CACHE_EXT)
}

fn format_local(local: IVec3) -> String {
    local
        .to_string()
        .chars()
        .filter_map(|c| match c {
            ',' => Some('_'),
            ' ' | '[' | ']' => None,
            _ => Some(c),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs::remove_file;

    use super::*;

    #[test]
    fn update_voxel() {
        let mut world = VoxWorld::default();
        let local = (0, 0, 0).into();
        world.add(local, ChunkKind::default());

        let voxels = vec![
            ((0, 0, 0).into(), 1.into()),
            ((1, 1, 1).into(), 2.into()),
            ((0, chunk::Y_END as i32, 5).into(), 3.into()),
        ];

        let dirty_chunks = super::update_voxel(&mut world, local, &voxels);

        let chunk = world.get(local).unwrap();

        assert_eq!(chunk.get((0, 0, 0).into()), 1.into());
        assert_eq!(chunk.get((1, 1, 1).into()), 2.into());
        assert_eq!(chunk.get((0, chunk::Y_END as i32, 5).into()), 3.into());

        assert_eq!(
            dirty_chunks.len(),
            5,
            "Should have 5 dirty chunks = central, left, down, back and up chunk. Currently {:?}",
            dirty_chunks
        );
    }

    #[test]
    fn unload_chunk() {
        let local = (9111, -9222, 9333).into();
        let mut world = VoxWorld::default();

        world.add(local, ChunkKind::default());

        let dirty_chunks = super::unload_chunk(&mut world, local);

        assert_eq!(dirty_chunks.len(), super::voxel::SIDE_COUNT);
        assert!(
            world.get(local).is_none(),
            "Chunk should be removed from world"
        );
    }

    #[test]
    fn load_chunk() {
        // Load existing cache
        let local = (9943, 9943, 9999).into();
        let path = super::local_path(local);
        let chunk = ChunkKind::default();

        create_cache(&path, &ChunkCache { local, kind: chunk });

        let mut world = VoxWorld::default();

        let dirty_chunks = super::load_chunk(&mut world, local);

        assert_eq!(dirty_chunks.len(), super::voxel::SIDE_COUNT + 1);
        assert!(dirty_chunks.contains(&local));
        assert!(world.get(local).is_some(), "Chunk should be added to world");

        let _ = remove_file(path);

        // Load non-existing cache
        let local = (9942, 9944, 9421).into();
        let path = super::local_path(local);
        let _ = remove_file(&path);

        let mut world = VoxWorld::default();
        let dirty_chunks = super::load_chunk(&mut world, local);

        assert_eq!(dirty_chunks.len(), super::voxel::SIDE_COUNT + 1);
        assert!(dirty_chunks.contains(&local));
        assert!(path.exists(), "Cache file should be created by load_chunk");
        assert!(world.get(local).is_some(), "Chunk should be added to world");

        let _ = remove_file(path);
    }

    #[test]
    fn update_chunk() {
        let mut world = VoxWorld::default();
        assert!(
            !super::update_chunk(&mut world, (0, 0, 0).into()),
            "should return false when chunk doesn't exists"
        );

        world.add((0, 0, 0).into(), ChunkKind::default());
        world.add((0, 1, 0).into(), ChunkKind::default());

        assert!(
            super::update_chunk(&mut world, (0, 0, 0).into()),
            "should return true when chunk doesn't exists"
        );

        let chunk = world.get((0, 0, 0).into()).unwrap();
        assert!(
            chunk
                .neighborhood
                .get(super::voxel::Side::Up, (0, 0, 0).into())
                .is_some(),
            "Neighborhood should be updated on update_chunk call"
        );
    }

    #[test]
    fn generate_cache() {
        let local = (5432, 4321, 5555).into();
        let cache = super::generate_cache(local);
        let path = local_path(local);

        assert!(path.exists(), "Generate cache should save cache on disk");
        assert_eq!(
            cache.local, local,
            "Generate cache should have the same local as the given one"
        );

        remove_file(path).expect("File should exists");
    }

    #[test]
    #[should_panic]
    fn generate_cache_panic() {
        let local = (9999, 9998, 9997).into();
        let _ = remove_file(local_path(local));

        super::generate_cache(local);
        super::generate_cache(local);
    }

    #[test]
    fn local_path_test() {
        let path = super::local_path((0, 0, 0).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("0_0_0.{}", CACHE_EXT)));

        let path = super::local_path((-1, 0, 0).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("-1_0_0.{}", CACHE_EXT)));

        let path = super::local_path((-1, 3333, -461).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("-1_3333_-461.{}", CACHE_EXT)));
    }

    #[test]
    fn test_ser_de() {
        let mut temp_file = std::env::temp_dir();
        temp_file.push("test.tmp");

        let cache = ChunkCache {
            local: IVec3::ZERO,
            kind: ChunkKind::default(),
        };

        create_cache(&temp_file, &cache);

        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(&temp_file)
            .unwrap();

        #[cfg(feature = "serde_ron")]
        let cache_loaded: ChunkCache = ron::de::from_reader(file).unwrap();

        #[cfg(not(feature = "serde_ron"))]
        let cache_loaded: ChunkCache = bincode::deserialize_from(file).unwrap();

        assert_eq!(cache, cache_loaded);
    }

    fn create_cache(path: &Path, cache: &ChunkCache) {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();

        #[cfg(feature = "serde_ron")]
        ron::ser::to_writer(file, cache).unwrap();

        #[cfg(not(feature = "serde_ron"))]
        bincode::serialize_into(file, cache).unwrap();
    }

    #[test]
    fn format_local() {
        assert_eq!("-234_22_1", super::format_local((-234, 22, 1).into()));
        assert_eq!(
            "-9999_-9999_-9999",
            super::format_local((-9999, -9999, -9999).into())
        );
        assert_eq!(
            "9999_-9999_9999",
            super::format_local((9999, -9999, 9999).into())
        );
        assert_eq!("0_0_0", super::format_local((0, 0, 0).into()));
    }

    #[test]
    fn load_cache() {
        let local = (-9998, 0, 9998).into();

        let cache = ChunkCache {
            local,
            kind: ChunkKind::default(),
        };

        let path = local_path(local);
        create_cache(&path, &cache);

        let loaded_cache = super::load_cache(&path);

        assert_eq!(cache, loaded_cache);

        remove_file(path).unwrap();
    }

    #[test]
    fn save_cache() {
        let local = (-921, 0, 2319).into();

        let cache = ChunkCache {
            local,
            kind: ChunkKind::default(),
        };

        let path = local_path(local);

        assert!(!path.exists());

        super::save_cache(&path, &cache);

        assert!(path.exists());

        let loaded_cache = super::load_cache(&path);

        assert_eq!(cache, loaded_cache);

        remove_file(path).unwrap();
    }

    #[test]
    fn optimize_commands_preserve_insertion_order() {
        let cmds = (0..100)
            .into_iter()
            .map(|i| ChunkCmd::Load((i, i, i).into()))
            .collect::<Vec<_>>();

        let optmized = super::optimize_commands(&VoxWorld::default(), cmds.clone());

        assert_eq!(cmds, optmized);
    }

    #[test]
    fn optimize_commands_rule_1() {
        let cmds = vec![
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Load((1, 2, 1).into()),
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Load((1, 2, 1).into()),
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Load((1, 3, 1).into()),
            ChunkCmd::Load((1, 2, 1).into()),
        ];
        let world = VoxWorld::default();

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(
            optmized,
            vec![
                ChunkCmd::Load((1, 1, 1).into()),
                ChunkCmd::Load((1, 2, 1).into()),
                ChunkCmd::Load((1, 3, 1).into()),
            ]
        );
    }

    #[test]
    fn optimize_commands_rule_2() {
        let cmds = vec![
            ChunkCmd::Unload((1, 1, 1).into()),
            ChunkCmd::Load((1, 1, 1).into()),
        ];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optmized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_3() {
        let cmds = vec![
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Unload((1, 1, 1).into()),
        ];
        let world = VoxWorld::default();

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optmized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_4() {
        let cmds = vec![ChunkCmd::Unload((1, 1, 1).into())];
        let world = VoxWorld::default();

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optmized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_5() {
        let cmds = vec![ChunkCmd::Load((1, 1, 1).into())];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optmized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_6() {
        let cmds = vec![ChunkCmd::Update((1, 1, 1).into(), vec![])];
        let world = VoxWorld::default();

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optmized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_7() {
        let cmds = vec![
            ChunkCmd::Update((1, 1, 1).into(), vec![]),
            ChunkCmd::Unload((1, 1, 1).into()),
        ];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optmized, vec![ChunkCmd::Unload((1, 1, 1).into())]);
    }

    #[test]
    fn optimize_commands_rule_9() {
        let cmds = vec![
            ChunkCmd::Unload((1, 1, 1).into()),
            ChunkCmd::Update((1, 1, 1).into(), vec![]),
        ];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optmized, vec![ChunkCmd::Unload((1, 1, 1).into())]);
    }

    #[test]
    fn optimize_commands_all_rules() {
        let cmds = vec![
            ChunkCmd::Load((0, 0, 0).into()),
            ChunkCmd::Load((1, 1, 1).into()),   // Skipped by Rule 1
            ChunkCmd::Unload((1, 1, 1).into()), // Removed by Rule 2
            ChunkCmd::Load((1, 1, 1).into()),   // Skipped by Rule 2
            ChunkCmd::Update((1, 1, 1).into(), vec![]),
            ChunkCmd::Load((1, 2, 1).into()),   // Removed by Rule 3
            ChunkCmd::Unload((1, 2, 1).into()), // Skipped by Rule 3
            ChunkCmd::Unload((1, 2, 1).into()), // Skipped by rule 4
            ChunkCmd::Load((1, 3, 1).into()),   // Skipped by Rule 5
            ChunkCmd::Update((1, 4, 1).into(), vec![]), // Skipped by Rule 6
            ChunkCmd::Update((1, 5, 1).into(), vec![]), // Replaced by Rule 7
            ChunkCmd::Update((1, 5, 1).into(), vec![]), // Replaced by Rule 1
            ChunkCmd::Update((1, 5, 1).into(), vec![]), // Replaced by Rule 1
            ChunkCmd::Unload((1, 5, 1).into()),
            ChunkCmd::Unload((1, 6, 1).into()),
            ChunkCmd::Update((1, 6, 1).into(), vec![]), // Skipped by Rule 9
        ];

        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());
        world.add((1, 3, 1).into(), Default::default());
        world.add((1, 5, 1).into(), Default::default());
        world.add((1, 6, 1).into(), Default::default());

        let optmized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(
            optmized,
            vec![
                ChunkCmd::Load((0, 0, 0).into()),
                ChunkCmd::Update((1, 1, 1).into(), vec![]),
                ChunkCmd::Unload((1, 5, 1).into()),
                ChunkCmd::Unload((1, 6, 1).into())
            ]
        );
    }
}
