use std::{
    io::{Read, Write},
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

use crate::world::{
    math, mesh,
    rendering::ChunkFacesOcclusion,
    storage::{
        chunk::{self, Chunk, ChunkKind, ChunkNeighborhood},
        voxel::{self, FacesOcclusion, VoxelFace, VoxelVertex},
        VoxWorld,
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
            .add_startup_system(setup_resources)
            .add_system(update_world_system);
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

**Returns** an optimized command list
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

type VoxelUpdateList = Vec<(IVec3, voxel::Kind)>;

/**
Utility function that splits the given list of [`ChunkCmd`] into individual cmd lists

***Returns*** tuple with load, unload and update cmd lists
 */
fn split_commands(
    commands: Vec<ChunkCmd>,
) -> (Vec<IVec3>, Vec<IVec3>, Vec<(IVec3, VoxelUpdateList)>) {
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
Process in batch a list of [`ChunkCmd`]. This function takes ownership of [`VoxWorld`] since it needs to do modification on world.

***Returns*** the [`VoxWorld`] ownership and a list of [`ChunkCmdResult`]
 */
fn process_batch(mut world: VoxWorld, commands: Vec<ChunkCmd>) -> (VoxWorld, Vec<ChunkCmdResult>) {
    let mut _perf = perf_fn!();

    let commands = optimize_commands(&world, commands);
    let (load, unload, update) = split_commands(commands);

    let mut dirty_chunks = unload_chunks(&mut world, &unload);
    dirty_chunks.extend(load_chunks(&mut world, &load));
    dirty_chunks.extend(update_chunks(&mut world, &update));

    let updated = refresh_chunks(&mut world, dirty_chunks.into_iter());

    let result = load
        .into_iter()
        .map(ChunkCmdResult::Loaded)
        .chain(unload.into_iter().map(ChunkCmdResult::Unloaded))
        .chain(updated.into_iter().map(ChunkCmdResult::Updated))
        .collect();

    (world, result)
}

/**
Apply on the given [`VoxWorld`] the given voxel modification list [`VoxelUpdateList`]

***Returns*** A list of chunks locals that are dirty due to voxel modifications. This is usually neighboring chunks where voxel was updated
 */
fn update_chunks(world: &mut VoxWorld, data: &[(IVec3, VoxelUpdateList)]) -> HashSet<IVec3> {
    let mut dirty_chunks = HashSet::default();

    for (local, voxels) in data {
        trace!("Updating chunk {} values {:?}", local, voxels);
        if let Some(chunk) = world.get_mut(*local) {
            for (voxel, kind) in voxels {
                chunk.kinds.set(*voxel, *kind);

                if chunk::is_at_bounds(*voxel) {
                    let neighbor_dir = chunk::get_boundary_dir(*voxel);
                    for unit_dir in math::to_unit_dir(neighbor_dir) {
                        let neighbor = unit_dir + *local;
                        dirty_chunks.insert(neighbor);
                    }
                }
            }

            dirty_chunks.insert(*local);
        } else {
            warn!("Failed to set voxel. Chunk {} wasn't found.", local);
        }
    }

    dirty_chunks
}

/**
Remove from [`VoxWorld`] all chunks on the given list.

***Returns*** A list of chunks locals that are dirty due to neighboring chunks removal.
 */
fn unload_chunks(world: &mut VoxWorld, locals: &[IVec3]) -> HashSet<IVec3> {
    perf_fn_scope!();

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

***Returns*** A list of chunks locals that are dirty due to neighboring chunks loading.
 */
fn load_chunks(world: &mut VoxWorld, locals: &[IVec3]) -> HashSet<IVec3> {
    perf_fn_scope!();

    let mut dirty_chunks = HashSet::default();

    for &local in locals {
        let path = local_path(local);

        let chunk = if path.exists() {
            load_chunk(&path)
        } else {
            generate_chunk(local)
        };

        world.add(local, chunk);

        dirty_chunks.extend(
            voxel::SIDES
                .iter()
                .map(|s| s.dir() + local)
                .chain(std::iter::once(local)),
        );
    }

    dirty_chunks
}

/**
Refresh chunks internal data due to change in the neighborhood. At moment this function only refresh neighborhood data.

***Returns*** A list of chunks locals that was refreshed.
 */
fn refresh_chunks(world: &mut VoxWorld, locals: impl Iterator<Item = IVec3>) -> Vec<IVec3> {
    perf_fn_scope!();

    let mut result = vec![];

    for local in locals {
        update_neighborhood(world, local);

        if let Some(chunk) = world.get_mut(local) {
            chunk.vertices = {
                let occlusion = faces_occlusion(&chunk.kinds);

                if !occlusion.is_fully_occluded() {
                    let faces = mesh::merge_faces(&occlusion, &chunk);
                    generate_vertices(faces)
                } else {
                    vec![]
                }
            };

            result.push(local);
        }
    }

    result
}

/**
Generates vertices data from a given [`VoxelFace`] list.

All generated indices will be relative to a triangle list.

**Returns** a list of generated [`VoxelVertex`].
*/
fn generate_vertices(faces: Vec<VoxelFace>) -> Vec<VoxelVertex> {
    perf_fn_scope!();

    let mut vertices = vec![];

    for face in faces {
        let normal = face.side.normal();

        for (i, v) in face.vertices.iter().enumerate() {
            let base_vertex_idx = mesh::VERTICES_INDICES[face.side as usize][i];
            let base_vertex: Vec3 = mesh::VERTICES[base_vertex_idx].into();
            vertices.push(VoxelVertex {
                position: base_vertex + v.as_vec3(),
                normal,
            })
        }
    }

    vertices
}

/**
Computes the faces occlusion data of the given [`ChunkKind`]

**Returns** computed [`ChunkFacesOcclusion`]
*/
fn faces_occlusion(chunk: &ChunkKind) -> ChunkFacesOcclusion {
    perf_fn_scope!();

    let mut occlusion = ChunkFacesOcclusion::default();
    for voxel in chunk::voxels() {
        let mut voxel_faces = FacesOcclusion::default();

        if chunk.get(voxel).is_empty() {
            voxel_faces.set_all(true);
        } else {
            for side in voxel::SIDES {
                let dir = side.dir();
                let neighbor_pos = voxel + dir;

                let neighbor_kind = if !chunk::is_within_bounds(neighbor_pos) {
                    let (_, next_chunk_voxel) = chunk::overlap_voxel(neighbor_pos);

                    match chunk.neighborhood.get(side, next_chunk_voxel) {
                        Some(k) => k,
                        None => continue,
                    }
                } else {
                    chunk.get(neighbor_pos)
                };

                voxel_faces.set(side, !neighbor_kind.is_empty());
            }
        }

        occlusion.set(voxel, voxel_faces);
    }

    occlusion
}

/**
Updates the [`ChunkNeighborhood`] of a given chunk local.
This function updates any neighborhood data needed by chunk.

Currently it only updates kind neighborhood data, but in the future, it may update light and other relevant data.
*/
fn update_neighborhood(world: &mut VoxWorld, local: IVec3) {
    if !world.exists(local) {
        return;
    }

    let mut neighborhood = ChunkNeighborhood::default();
    for side in voxel::SIDES {
        let dir = side.dir();
        let neighbor = local + dir;

        if let Some(neighbor_chunk) = world.get(neighbor) {
            neighborhood.set(side, &neighbor_chunk.kinds);
        }
    }

    if let Some(chunk) = world.get_mut(local) {
        chunk.kinds.neighborhood = neighborhood;
    }
}

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

    let chunk = Chunk {
        kinds,
        ..Default::default()
    };
    save_chunk(&path, &chunk);

    chunk
}

fn save_chunk(path: &Path, chunk: &Chunk) {
    perf_fn_scope!();

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

    use super::*;

    #[test]
    fn update_voxel() {
        let mut world = VoxWorld::default();
        let local = (0, 0, 0).into();
        world.add(local, Default::default());

        let voxels = vec![
            ((0, 0, 0).into(), 1.into()),
            ((1, 1, 1).into(), 2.into()),
            ((0, chunk::Y_END as i32, 5).into(), 3.into()),
        ];

        let dirty_chunks = super::update_chunks(&mut world, &vec![(local, voxels)]);

        let kinds = &world.get(local).unwrap().kinds;

        assert_eq!(kinds.get((0, 0, 0).into()), 1.into());
        assert_eq!(kinds.get((1, 1, 1).into()), 2.into());
        assert_eq!(kinds.get((0, chunk::Y_END as i32, 5).into()), 3.into());

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

        world.add(local, Default::default());

        let dirty_chunks = super::unload_chunks(&mut world, &vec![local]);

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
        let kinds = ChunkKind::default();

        create_chunk(
            &path,
            &Chunk {
                kinds,
                ..Default::default()
            },
        );

        let mut world = VoxWorld::default();

        let dirty_chunks = super::load_chunks(&mut world, &vec![local]);

        assert_eq!(dirty_chunks.len(), super::voxel::SIDE_COUNT + 1);
        assert!(dirty_chunks.contains(&local));
        assert!(world.exists(local), "Chunk should be added to world");

        let _ = remove_file(path);

        // Load non-existing cache
        let local = (9942, 9944, 9421).into();
        let path = super::local_path(local);
        let _ = remove_file(&path);

        let mut world = VoxWorld::default();
        let dirty_chunks = super::load_chunks(&mut world, &vec![local]);

        assert_eq!(dirty_chunks.len(), super::voxel::SIDE_COUNT + 1);
        assert!(dirty_chunks.contains(&local));
        assert!(path.exists(), "Cache file should be created by load_chunk");
        assert!(world.exists(local), "Chunk should be added to world");

        let _ = remove_file(path);
    }

    #[test]
    fn update_chunk() {
        let mut world = VoxWorld::default();
        assert!(
            super::refresh_chunks(&mut world, [(0, 0, 0).into()].into_iter()).is_empty(),
            "should return an empty list when chunk doesn't exists"
        );

        world.add((0, 0, 0).into(), Default::default());
        world.add((0, 1, 0).into(), Default::default());

        assert!(
            super::refresh_chunks(&mut world, [(0, 0, 0).into()].into_iter()).len() == 1,
            "should return true when chunk doesn't exists"
        );

        let chunk = world.get((0, 0, 0).into()).unwrap();
        assert!(
            chunk
                .kinds
                .neighborhood
                .get(super::voxel::Side::Up, (0, 0, 0).into())
                .is_some(),
            "Neighborhood should be updated on update_chunk call"
        );
    }

    #[test]
    fn generate_chunk() {
        let local = (5432, 4321, 5555).into();
        let _ = super::generate_chunk(local);
        let path = local_path(local);

        assert!(path.exists(), "Generate chunk should save cache on disk");

        remove_file(path).expect("File should exists");
    }

    #[test]
    #[should_panic]
    fn generate_cache_panic() {
        let local = (9999, 9998, 9997).into();
        let _ = remove_file(local_path(local));

        super::generate_chunk(local);
        super::generate_chunk(local);
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

        let chunk = Default::default();

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

    #[test]
    fn update_neighborhood() {
        let mut world = VoxWorld::default();

        let center = (1, 1, 1).into();
        let mut chunk = Chunk::default();
        chunk.kinds.set_all(10.into());
        world.add(center, chunk);

        for side in voxel::SIDES {
            let dir = side.dir();
            let pos = center + dir;
            let mut chunk = Chunk::default();
            chunk.kinds.set_all((side as u16).into());
            world.add(pos, chunk);
        }

        super::update_neighborhood(&mut world, center);
        let chunk = world.get(center).unwrap();

        for side in voxel::SIDES {
            match side {
                voxel::Side::Right => {
                    for a in 0..chunk::Y_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (0, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Left => {
                    for a in 0..chunk::Y_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (chunk::X_END as i32, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Up => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, 0, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Down => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, chunk::Y_END as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Front => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Y_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, b as i32, 0).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Back => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Y_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, b as i32, chunk::Z_END as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn faces_occlusion_occlude_empty_chunk() {
        // Arrange
        let chunk = ChunkKind::default();

        // Act
        let occlusions = super::faces_occlusion(&chunk);

        // Assert
        assert!(
            occlusions.iter().all(|a| a.is_fully_occluded()),
            "A chunk full of empty-kind voxels should be fully occluded"
        );
    }

    #[test]
    fn faces_occlusion() {
        // Arrange
        let mut chunk = ChunkKind::default();

        // Top-Bottom occlusion
        chunk.set((1, 1, 1).into(), 1.into());
        chunk.set((1, 2, 1).into(), 1.into());

        // Full occluded voxel at (10, 10, 10)
        chunk.set((10, 10, 10).into(), 1.into());
        chunk.set((9, 10, 10).into(), 1.into());
        chunk.set((11, 10, 10).into(), 1.into());
        chunk.set((10, 9, 10).into(), 1.into());
        chunk.set((10, 11, 10).into(), 1.into());
        chunk.set((10, 10, 9).into(), 1.into());
        chunk.set((10, 10, 11).into(), 1.into());

        // Act
        let faces_occlusion = super::faces_occlusion(&chunk);

        // Assert
        let faces = faces_occlusion.get((1, 2, 1).into());

        assert_eq!(
            faces,
            [false, false, false, true, false, false].into(),
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.get((1, 1, 1).into());

        assert_eq!(
            faces,
            [false, false, true, false, false, false].into(),
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.get((10, 10, 10).into());

        assert_eq!(
            faces,
            [true; voxel::SIDE_COUNT].into(),
            "Voxel fully surrounded by another non-empty voxels should be fully occluded"
        );
    }

    #[test]
    fn faces_occlusion_neighborhood() {
        let mut world = VoxWorld::default();

        let mut top = Chunk::default();
        top.kinds.set_all(2.into());

        let mut down = Chunk::default();
        down.kinds.set_all(3.into());

        let mut center = Chunk::default();
        center
            .kinds
            .set((0, chunk::Y_END as i32, 0).into(), 1.into());
        center.kinds.set((1, 0, 1).into(), 1.into());

        world.add((0, 1, 0).into(), top);
        world.add((0, 0, 0).into(), center);
        world.add((0, -1, 0).into(), down);

        super::update_neighborhood(&mut world, (0, 0, 0).into());
        let center = world.get((0, 0, 0).into()).unwrap();

        let faces_occlusion = super::faces_occlusion(&center.kinds);

        let faces = faces_occlusion.get((0, chunk::Y_END as i32, 0).into());
        assert_eq!(faces, [false, false, true, false, false, false].into());

        let faces = faces_occlusion.get((1, 0, 1).into());
        assert_eq!(faces, [false, false, false, true, false, false].into());
    }

    #[test]
    fn generate_vertices() {
        // Arrange
        let side = voxel::Side::Up;
        let faces = vec![VoxelFace {
            side,
            vertices: [
                (0, 0, 0).into(),
                (0, 0, 1).into(),
                (1, 0, 1).into(),
                (1, 0, 0).into(),
            ],
        }];

        // Act
        let vertices = super::generate_vertices(faces);

        // Assert
        let normal = side.normal();
        assert_eq!(
            vertices,
            vec![
                VoxelVertex {
                    normal,
                    position: (0.0, 1.0, 0.0).into(),
                },
                VoxelVertex {
                    normal,
                    position: (0.0, 1.0, 2.0).into(),
                },
                VoxelVertex {
                    normal,
                    position: (2.0, 1.0, 2.0).into(),
                },
                VoxelVertex {
                    normal,
                    position: (2.0, 1.0, 0.0).into(),
                },
            ]
        );
    }
}
