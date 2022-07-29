use std::{
    io::{Read, Write},
    ops::Deref,
    path::{Path, PathBuf},
};

use bevy::{
    prelude::*,
    reflect::TypeUuid,
    tasks::{AsyncComputeTaskPool, Task},
    utils::{HashMap, HashSet},
};
use bracket_noise::prelude::{FastNoise, FractalType, NoiseType};
use futures_lite::future;

use crate::world::storage::{
    chunk::{self, Chunk, ChunkKind, ChunkLight},
    voxel::{self, KindsDescs},
    VoxWorld,
};

use super::{shaping, VoxelUpdateList};

const CACHE_PATH: &str = "cache/chunks/";
const CACHE_EXT: &str = "bin";

pub(super) struct GenesisPlugin;

impl Plugin for GenesisPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkUpdated>()
            .add_startup_system_to_stage(StartupStage::PreStartup, setup_resources)
            .add_system(update_world_system);
    }
}

#[derive(TypeUuid, Debug)]
#[uuid = "e6edff2a-e204-497f-999c-bdebd1f92f62"]
pub struct KindsDescsRes {
    pub descs: KindsDescs,
    pub atlas: Handle<Image>,
}

pub struct EvtChunkUpdated(pub IVec3);

fn setup_resources(mut commands: Commands, asset_server: Res<AssetServer>) {
    trace_system_run!();

    if !std::path::Path::new(CACHE_PATH).exists() {
        std::fs::create_dir_all(CACHE_PATH).unwrap();
    }

    let vox_world = VoxWorld::default();
    commands.insert_resource(WorldRes(Some(vox_world)));

    // TODO: Find a better way to load this
    let input_path = format!("{}/assets/voxels/kind.ron", env!("CARGO_MANIFEST_DIR"));
    let f = std::fs::File::open(&input_path).expect("Failed opening kind descriptions file");
    let descs: KindsDescs = ron::de::from_reader(f).unwrap();

    let atlas = asset_server.load(&descs.atlas_path);

    commands.insert_resource(KindsDescsRes { descs, atlas });

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
        !self.pending.is_empty()
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
    running_task: Option<Task<(VoxWorld, Vec<IVec3>)>>,
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
    kind_assets: Res<KindsDescsRes>,
    mut batch_res: ResMut<BatchChunkCmdRes>,
    mut meta: Local<ProcessBatchSystemMeta>,
    mut world_res: ResMut<WorldRes>,
    mut updated_writer: EventWriter<EvtChunkUpdated>,
) {
    let mut _perf = perf_fn!();

    if let Some(ref mut task) = meta.running_task {
        perf_scope!(_perf);

        // Check if task has finished
        if let Some((world, updated_list)) = future::block_on(future::poll_once(task)) {
            // Dispatch all events generated from this batch
            updated_list
                .into_iter()
                .for_each(|local| updated_writer.send(EvtChunkUpdated(local)));

            // Give back the VoxWorld to WorldRes
            meta.running_task = None;
            world_res.set(world);
            batch_res.finished();
        }
    } else if batch_res.has_pending_commands() {
        perf_scope!(_perf);

        let world = world_res.take();
        let kinds_descs = kind_assets.descs.clone();
        let batch = batch_res.swap_and_clone();

        meta.running_task =
            Some(task_pool.spawn(async move { process_batch(world, kinds_descs, batch) }));
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

**Returns** an optimized command list
*/
fn optimize_commands(world: &VoxWorld, commands: Vec<ChunkCmd>) -> Vec<ChunkCmd> {
    perf_fn_scope!();

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
                                "Undefined behavior for {:?} and {:?} when chunk_exists = {:?}",
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
                                "Undefined behavior for {:?} and {:?} when chunk_exists = {:?}",
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
                            panic!("Undefined behavior for {:?} and {:?}", cmd, existing_cmd);
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

/**
Utility function that splits the given list of [`ChunkCmd`] into individual cmd lists

***Returns*** tuple with load, unload and update cmd lists
 */
fn split_commands(
    commands: Vec<ChunkCmd>,
) -> (Vec<IVec3>, Vec<IVec3>, Vec<(IVec3, VoxelUpdateList)>) {
    perf_fn_scope!();

    let mut load = vec![];
    let mut unload = vec![];
    let mut update = vec![];

    for cmd in commands.into_iter() {
        match cmd {
            ChunkCmd::Load(local) => load.push(local),
            ChunkCmd::Unload(local) => unload.push(local),
            ChunkCmd::Update(local, data) => update.push((local, data)),
        }
    }

    (load, unload, update)
}

/**
Process a batch a list of [`ChunkCmd`]. This function takes ownership of [`VoxWorld`] since it needs to do modification on world.

This function triggers [`recompute_chunks`] whenever a new chunk is generated or is updated.

***Returns*** the [`VoxWorld`] ownership and a list of updated chunks.
 */
fn process_batch(
    mut world: VoxWorld,
    kinds_descs: KindsDescs,
    commands: Vec<ChunkCmd>,
) -> (VoxWorld, Vec<IVec3>) {
    let mut _perf = perf_fn!();

    let commands = optimize_commands(&world, commands);
    let (load, unload, update) = split_commands(commands);

    unload_chunks(&mut world, &unload);

    let not_found = load_chunks(&mut world, &load);

    let mut updated = generate_chunks(&mut world, not_found, &kinds_descs)
        .into_iter()
        .collect::<HashSet<_>>();

    trace!("Generation completed! {} chunks updated.", updated.len());

    updated.extend(update_chunks(&mut world, &update, &kinds_descs));

    // let dirty_chunks = dirty_chunks.into_iter().collect::<Vec<_>>();
    // let updated = recompute_chunks(&mut world, kinds_descs, dirty_chunks);

    (world, updated.into_iter().collect())
}

/**
Applies on the given [`VoxWorld`] a voxel modification list [`VoxelUpdateList`]

***Returns*** A list of chunks locals that are dirty due to voxel modifications. This is usually neighboring chunks where voxel was updated
 */
fn update_chunks(
    world: &mut VoxWorld,
    update_list: &[(IVec3, VoxelUpdateList)],
    kinds_descs: &KindsDescs,
) -> Vec<IVec3> {
    perf_fn_scope!();

    let mut recompute_map = HashMap::default();

    // Apply modifications and keep track of what chunks needs to be recomputed
    for (local, voxels) in update_list {
        if let Some(chunk) = world.get_mut(*local) {
            recompute_map.insert(*local, voxels.iter().cloned().collect());

            trace!("Updating chunk {} values {:?}", local, voxels);

            for &(voxel, kind) in voxels {
                chunk.kinds.set(voxel, kind);

                // If this updates happens at the edge of chunk, mark neighbors chunk as dirty, since this will likely affect'em
                recompute_map.extend(
                    chunk::neighboring(*local, voxel)
                        .into_iter()
                        // There is no voxel to update, just recompute neighbor internals
                        .map(|neighbor_local| (neighbor_local, vec![])),
                );
            }
        } else {
            warn!("Failed to set voxel. Chunk {} wasn't found.", local);
        }
    }

    let updated_chunks = recompute_map.into_iter().collect::<Vec<_>>();
    recompute_chunks_internals(world, kinds_descs, &updated_chunks)
}

/**
Remove from [`VoxWorld`] all chunks on the given list.

***Returns*** A list of chunks locals that are dirty due to neighboring chunks removal.
 */
fn unload_chunks(world: &mut VoxWorld, locals: &[IVec3]) -> HashSet<IVec3> {
    let mut dirty_chunks = HashSet::default();

    for &local in locals {
        if world.remove(local).is_none() {
            warn!("Trying to unload non-existing chunk {}", local);
        } else {
            dirty_chunks.extend(voxel::SIDES.map(|s| s.dir() + local))
        }
    }

    dirty_chunks
}

/**
Load from cache into [`VoxWorld`] all chunks on the given list.

***Returns*** A list of chunks locals which doesn't exists on cache.
 */
fn load_chunks(world: &mut VoxWorld, locals: &[IVec3]) -> Vec<IVec3> {
    locals
        .iter()
        .filter_map(|local| {
            let path = local_path(*local);
            if path.exists() {
                world.add(*local, load_chunk(&path));
                None
            } else {
                Some(*local)
            }
        })
        .collect()
}

/**
Refresh chunks internal data due to change in the chunk itself or neighborhood.

***Returns*** A list of chunks locals that was refreshed.
 */
fn recompute_chunks_internals(
    world: &mut VoxWorld,
    kinds_descs: &KindsDescs,
    update: &[(IVec3, VoxelUpdateList)],
) -> Vec<IVec3> {
    perf_fn_scope!();

    let locals = shaping::recompute_chunks_internals(world, &kinds_descs, update);

    // TODO: Find a way to only saving chunks which was really updated.
    for &local in locals.iter() {
        let path = local_path(local);
        save_chunk(&path, world.get(local).unwrap());
    }

    locals
}

/**
Refresh chunks internal data due to change in the neighborhood. At moment this function only refresh neighborhood data.

***Returns*** A list of chunks locals that was refreshed.
 */
fn compute_chunks_internals(
    world: &mut VoxWorld,
    kinds_descs: &KindsDescs,
    locals: Vec<IVec3>,
) -> Vec<IVec3> {
    perf_fn_scope!();

    let locals = shaping::compute_chunks_internals(world, &kinds_descs, locals);

    trace!("Saving {} chunks on disk!", locals.len());

    // TODO: Find a way to only saving chunks which was really updated.
    for &local in locals.iter() {
        let path = local_path(local);
        save_chunk(&path, world.get(local).unwrap());
    }

    locals
}

fn generate_chunks(
    world: &mut VoxWorld,
    locals: Vec<IVec3>,
    kinds_descs: &KindsDescs,
) -> Vec<IVec3> {
    trace!("Generating {} chunks.", locals.len());

    // Before doing anything else, all generated chunks have to be added to world.
    locals.iter().for_each(|&local| {
        world.add(local, generate_chunk(local));
    });

    // Mark all generated chunks and it's surround as dirty
    let dirty_chunks = locals
        .iter()
        .flat_map(|&local| {
            voxel::SIDES
                .iter()
                .map(move |s| s.dir() + local)
                .chain(std::iter::once(local))
        })
        .collect::<HashSet<_>>() // Remove duplicated chunks
        .into_iter()
        .collect();

    compute_chunks_internals(world, kinds_descs, dirty_chunks)
}

/**
 Generates a new chunk filling it with [`ChunkKind`] randomly generated by seeded noise
*/
fn generate_chunk(local: IVec3) -> Chunk {
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
    let mut lights = ChunkLight::default();

    for x in 0..chunk::X_AXIS_SIZE {
        for z in 0..chunk::Z_AXIS_SIZE {
            lights.set(
                (x as i32, chunk::Y_END, z as i32).into(),
                voxel::Light::natural(voxel::Light::MAX_NATURAL_INTENSITY),
            );

            let h = noise.get_noise(world.x + x as f32, world.z + z as f32);
            let world_height = ((h + 1.0) / 2.0) * (chunk::X_AXIS_SIZE * 2) as f32;

            let height_local = world_height - world.y;

            if height_local < f32::EPSILON {
                continue;
            }

            let end = usize::min(height_local as usize, chunk::Y_AXIS_SIZE);

            for y in 0..end {
                // TODO: Check this following biome settings
                let kind = match y {
                    y if y == end - 1 => 2.into(),
                    y if y < end - 3 => 3.into(),
                    _ => 1.into(),
                };

                kinds.set((x as i32, y as i32, z as i32).into(), kind);
            }
        }
    }

    Chunk {
        kinds,
        lights,
        ..Default::default()
    }
}

/**
 Saves the given [`Chunk`] on disk at [`Path`].
*/
fn save_chunk(path: &Path, chunk: &Chunk) {
    perf_fn_scope!();

    // TODO: Change this to an async version?

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to write to file {}", path.display()));

    #[cfg(not(feature = "serde_ron"))]
    {
        let bincode = bincode::serialize(chunk)
            .unwrap_or_else(|_| panic!("Failed to serialize cache {}", path.display()));

        let compressed = lz4_flex::compress_prepend_size(&bincode);

        file.write_all(&compressed).unwrap_or_else(|_| {
            panic!(
                "Failed to write compressed data to cache {}",
                path.display()
            )
        });
    }

    #[cfg(feature = "serde_ron")]
    ron::ser::to_writer(file, chunk)
        .unwrap_or_else(|_| panic!("Failed to serialize cache to file {}", path.display()));
}

fn load_chunk(path: &Path) -> Chunk {
    perf_fn_scope!();

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to open file {}", path.display()));

    #[cfg(not(feature = "serde_ron"))]
    {
        let mut compressed = Vec::new();
        file.read_to_end(&mut compressed)
            .unwrap_or_else(|_| panic!("Failed to read file {}", path.display()));

        let decompressed = lz4_flex::decompress_size_prepended(&compressed)
            .unwrap_or_else(|_| panic!("Failed to decompress cache {}", path.display()));

        let chunk = bincode::deserialize(&decompressed)
            .unwrap_or_else(|_| panic!("Failed to parse file {}", path.display()));

        chunk
    }

    #[cfg(feature = "serde_ron")]
    {
        let cache =
            ron::de::from_reader(file).expect(&format!("Failed to parse file {}", path.display()));
        cache
    }
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

    use crate::world::storage::voxel::{KindDescItem, Light};

    use super::*;

    fn create_kinds_descs() -> KindsDescs {
        KindsDescs {
            atlas_path: "".into(),
            atlas_size: 320,
            atlas_tile_size: 32,
            descriptions: (0..100u16)
                .map(|i| KindDescItem {
                    name: format!("Kind {i}"),
                    id: i,
                    sides: voxel::KindSidesDesc::All(Default::default()),
                })
                .collect::<Vec<_>>(),
        }
    }

    fn top_voxels() -> impl Iterator<Item = IVec3> {
        (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| (x, chunk::Y_END, z).into()))
    }

    fn set_natural_light_on_top_voxels(chunk: &mut Chunk) {
        let light = Light::natural(Light::MAX_NATURAL_INTENSITY);

        for local in top_voxels() {
            chunk.lights.set(local, light);
        }
    }

    fn create_test_world() -> VoxWorld {
        /*
                           Chunk               Neighbor
                        +----+----+        +----+----+----+
                     11 | -- | 15 |        | -- | -- | 15 |
                        +----+----+        +----+----+----+
                     10 | -- | -- |        | -- | -- | 15 |
                        +----+----+        +----+----+----+
                     9  | -- | -- |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+
                     8  | -- | 2  |        | 1  | -- | 15 |
                        +----+----+        +----+----+----+
                     7  | -- | 3  |        | -- | -- | 15 |
                        +----+----+        +----+----+----+
                     6  | -- | 4  |        | 5  | -- | 15 |
                        +----+----+        +----+----+----+
                     5  | -- | -- |        | 6  | -- | 15 |
                        +----+----+        +----+----+----+
                     4  | -- | 8  |        | 7  | -- | 15 |
                        +----+----+        +----+----+----+
                     3  | -- | 9  |        | -- | -- | 15 |
                        +----+----+        +----+----+----+
        Y            2  | -- | 10 |        | 11 | -- | 15 |
        |               +----+----+        +----+----+----+
        |            1  | -- | -- |        | 12 | -- | 15 |
        + ---- X        +----+----+        +----+----+----+
                     0  | -- | 12 |        | 13 | 14 | 15 |
                        +----+----+        +----+----+----+

                     +    14   15            0    1    2
        */

        let mut world = VoxWorld::default();

        let mut chunk = Chunk::default();
        chunk.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (11..=chunk::Y_END).rev() {
            chunk.kinds.set((15, y, 0).into(), 0.into());
        }

        let mut neighbor = Chunk::default();
        neighbor.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (0..=chunk::Y_END).rev() {
            neighbor.kinds.set((2, y, 0).into(), 0.into());
        }

        chunk.kinds.set((15, 11, 0).into(), 0.into());
        chunk.kinds.set((15, 8, 0).into(), 0.into());
        chunk.kinds.set((15, 7, 0).into(), 0.into());
        chunk.kinds.set((15, 6, 0).into(), 0.into());
        chunk.kinds.set((15, 4, 0).into(), 0.into());
        chunk.kinds.set((15, 3, 0).into(), 0.into());
        chunk.kinds.set((15, 2, 0).into(), 0.into());
        chunk.kinds.set((15, 0, 0).into(), 0.into());

        neighbor.kinds.set((0, 8, 0).into(), 0.into());
        neighbor.kinds.set((0, 9, 0).into(), 0.into());
        neighbor.kinds.set((0, 6, 0).into(), 0.into());
        neighbor.kinds.set((0, 5, 0).into(), 0.into());
        neighbor.kinds.set((0, 4, 0).into(), 0.into());
        neighbor.kinds.set((0, 2, 0).into(), 0.into());
        neighbor.kinds.set((0, 1, 0).into(), 0.into());
        neighbor.kinds.set((0, 0, 0).into(), 0.into());
        neighbor.kinds.set((1, 0, 0).into(), 0.into());
        neighbor.kinds.set((2, 0, 0).into(), 0.into());

        set_natural_light_on_top_voxels(&mut neighbor);
        set_natural_light_on_top_voxels(&mut chunk);

        world.add((0, 0, 0).into(), chunk);
        world.add((1, 0, 0).into(), neighbor);

        let _ = super::shaping::compute_chunks_internals(
            &mut world,
            &create_kinds_descs(),
            vec![(0, 0, 0).into(), (1, 0, 0).into()],
        );

        let chunk = world.get((0, 0, 0).into()).unwrap();
        assert_eq!(chunk.lights.get_natural((15, 6, 0).into()), 4, "Failed to compute chunk internals. This is likely a bug handled by others tests. Ignore this and fix others.");

        let neighbor = world.get((1, 0, 0).into()).unwrap();
        assert_eq!(neighbor.lights.get_natural((0, 6, 0).into()), 5, "Failed to compute chunk internals. This is likely a bug handled by others tests. Ignore this and fix others.");

        world
    }

    #[test]
    fn update_chunks_neighbor_side_light() {
        let mut world = create_test_world();

        let update_list = [((0, 0, 0).into(), vec![((15, 10, 0).into(), 0.into())])];

        let descs = create_kinds_descs();
        let updated = super::update_chunks(&mut world, &update_list, &descs);

        assert_eq!(
            updated.len(),
            2,
            "A voxel was updated on the chunk edge, so there should be 2 updated chunks."
        );

        let chunk = world.get((0, 0, 0).into()).unwrap();

        assert_eq!(
            chunk.kinds.get((15, 10, 0).into()),
            0.into(),
            "Voxel should be updated to new kind"
        );

        assert_eq!(
            chunk.lights.get_natural((15, 10, 0).into()),
            Light::MAX_NATURAL_INTENSITY,
            "Voxel should have a natural light propagated to it"
        );

        let neighbor = world.get((1, 0, 0).into()).unwrap();

        // Get the vertices facing the updated voxel on the neighbor
        let updated_voxel_side_vertex = neighbor
            .vertices
            .iter()
            .find(|&v| v.normal == -Vec3::X && v.position == (0.0, 10.0, 0.0).into());

        assert!(
            updated_voxel_side_vertex.is_some(),
            "There should be a vertex for left side on updated voxel"
        );

        let updated_voxel_side_vertex = updated_voxel_side_vertex.unwrap();
        assert_eq!(updated_voxel_side_vertex.light, Vec3::ONE);
    }

    #[test]
    fn update_chunks_simple() {
        let mut world = VoxWorld::default();
        let local = (0, 0, 0).into();
        world.add(local, Default::default());

        let voxels = vec![
            ((0, 0, 0).into(), 1.into()),
            ((1, 1, 1).into(), 2.into()),
            ((0, chunk::Y_END as i32, 5).into(), 3.into()),
        ];

        let dirty_chunks =
            super::update_chunks(&mut world, &vec![(local, voxels)], &create_kinds_descs());

        let kinds = &world.get(local).unwrap().kinds;

        assert_eq!(kinds.get((0, 0, 0).into()), 1.into());
        assert_eq!(kinds.get((1, 1, 1).into()), 2.into());
        assert_eq!(kinds.get((0, chunk::Y_END as i32, 5).into()), 3.into());

        assert_eq!(dirty_chunks.len(), 1, "Should have 1 dirty chunks",);
    }

    #[test]
    fn unload_chunk() {
        let local = (9111, -9222, 9333).into();
        let mut world = VoxWorld::default();

        world.add(local, Default::default());

        let dirty_chunks = super::unload_chunks(&mut world, &vec![local]);

        assert_eq!(dirty_chunks.len(), super::voxel::SIDE_COUNT);
        assert!(
            world.get(local).is_none(),
            "Chunk should be removed from world"
        );
    }

    #[test]
    fn load_chunks() {
        // Load existing cache
        let local = (9943, 9943, 9999).into();
        let path = super::local_path(local);
        let chunk = Chunk::default();

        create_chunk(&path, &chunk);

        let mut world = VoxWorld::default();

        let dirty_chunks = super::load_chunks(&mut world, &vec![local]);

        assert_eq!(
            dirty_chunks.len(),
            0,
            "The chunk already exists, so no dirty chunks"
        );
        assert!(world.exists(local), "Chunk should be added to world");

        let _ = remove_file(path);

        // Load non-existing cache
        let local = (9942, 9944, 9421).into();

        let mut world = VoxWorld::default();
        let not_loaded = super::load_chunks(&mut world, &vec![local]);

        assert_eq!(
            not_loaded.len(),
            1,
            "Chunk doesn't exists, so it must be reported as not loaded"
        );
        assert!(not_loaded.contains(&local));
        assert!(!world.exists(local), "Chunk should not be added to world");
    }

    #[test]
    fn generate_chunk() {
        let local = (5432, 0, 5555).into();
        let chunk = super::generate_chunk(local);

        assert!(
            !chunk.kinds.is_default(),
            "Generate chunk should should not be default"
        );
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

        let mut chunk = Chunk::default();
        let mut neighbor = Chunk::default();
        neighbor.kinds.set_all(1.into());

        chunk
            .kinds
            .neighborhood
            .set(voxel::Side::Right, &neighbor.kinds);

        create_chunk(&temp_file, &chunk);

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .open(&temp_file)
            .unwrap();

        #[cfg(feature = "serde_ron")]
        let cache_loaded: ChunkCache = ron::de::from_reader(file).unwrap();

        #[cfg(not(feature = "serde_ron"))]
        {
            let mut compressed = Vec::new();
            file.read_to_end(&mut compressed).unwrap();
            let uncompressed = lz4_flex::decompress_size_prepended(&compressed).unwrap();
            let loaded_chunk = bincode::deserialize::<Chunk>(&uncompressed).unwrap();
            assert_eq!(chunk, loaded_chunk);

            assert_eq!(
                chunk
                    .kinds
                    .neighborhood
                    .get(voxel::Side::Right, (0, 0, 0).into()),
                Some(1.into())
            );
        }
    }

    fn create_chunk(path: &Path, chunk: &Chunk) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();

        #[cfg(feature = "serde_ron")]
        ron::ser::to_writer(file, chunk).unwrap();

        #[cfg(not(feature = "serde_ron"))]
        {
            let uncompressed = bincode::serialize(chunk).unwrap();
            let compressed = lz4_flex::compress_prepend_size(&uncompressed);
            file.write_all(&compressed).unwrap();
        }
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

        let chunk = Default::default();

        let path = local_path(local);
        create_chunk(&path, &chunk);

        let loaded_chunk = super::load_chunk(&path);

        assert_eq!(chunk, loaded_chunk);

        remove_file(path).unwrap();
    }

    #[test]
    fn save_cache() {
        let local = (-921, 0, 2319).into();

        let chunk = Default::default();

        let path = local_path(local);

        assert!(!path.exists());

        super::save_chunk(&path, &chunk);

        assert!(path.exists());

        let loaded_cache = super::load_chunk(&path);

        assert_eq!(chunk, loaded_cache);

        remove_file(path).unwrap();
    }

    #[test]
    fn optimize_commands_preserve_insertion_order() {
        let cmds = (0..100)
            .into_iter()
            .map(|i| ChunkCmd::Load((i, i, i).into()))
            .collect::<Vec<_>>();

        let optimized = super::optimize_commands(&VoxWorld::default(), cmds.clone());

        assert_eq!(cmds, optimized);
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

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(
            optimized,
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

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_3() {
        let cmds = vec![
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Unload((1, 1, 1).into()),
        ];
        let world = VoxWorld::default();

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_4() {
        let cmds = vec![ChunkCmd::Unload((1, 1, 1).into())];
        let world = VoxWorld::default();

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_5() {
        let cmds = vec![ChunkCmd::Load((1, 1, 1).into())];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_6() {
        let cmds = vec![ChunkCmd::Update((1, 1, 1).into(), vec![])];
        let world = VoxWorld::default();

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_7() {
        let cmds = vec![
            ChunkCmd::Update((1, 1, 1).into(), vec![]),
            ChunkCmd::Unload((1, 1, 1).into()),
        ];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![ChunkCmd::Unload((1, 1, 1).into())]);
    }

    #[test]
    fn optimize_commands_rule_9() {
        let cmds = vec![
            ChunkCmd::Unload((1, 1, 1).into()),
            ChunkCmd::Update((1, 1, 1).into(), vec![]),
        ];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![ChunkCmd::Unload((1, 1, 1).into())]);
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

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(
            optimized,
            vec![
                ChunkCmd::Load((0, 0, 0).into()),
                ChunkCmd::Update((1, 1, 1).into(), vec![]),
                ChunkCmd::Unload((1, 5, 1).into()),
                ChunkCmd::Unload((1, 6, 1).into())
            ]
        );
    }
}
