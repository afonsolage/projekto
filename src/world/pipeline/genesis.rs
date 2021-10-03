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

use crate::world::{
    math, mesh,
    storage::{
        chunk::{self, Chunk, ChunkKind, ChunkNeighborhood},
        voxel::{self, FacesOcclusion, VoxelFace, VoxelVertex},
        VoxWorld,
    },
};

use super::ChunkFacesOcclusion;

const CACHE_PATH: &str = "cache/chunks/example";
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

pub struct EvtChunkLoaded(pub IVec3);
pub struct EvtChunkUnloaded(pub IVec3);
pub struct EvtChunkUpdated(pub IVec3);

fn setup_resources(mut commands: Commands) {
    trace_system_run!();

    commands.insert_resource(WorldRes(Some(VoxWorld::default())));
    commands.insert_resource(BatchChunkCmdRes::default());
}

#[derive(Default)]
pub struct BatchChunkCmdRes {
    pending: HashMap<IVec3, ChunkCmd>,
    running: HashMap<IVec3, ChunkCmd>,
}

impl BatchChunkCmdRes {
    fn take(&mut self) -> HashMap<IVec3, ChunkCmd> {
        self.running = std::mem::take(&mut self.pending);
        self.running.clone()
    }

    fn finished(&mut self) {
        self.running.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn load(&mut self, local: IVec3) {
        if let Some(cmd) = self.running.get(&local) {
            warn!("Chunk {:?} cmd already exists: ", cmd);
            return;
        }
        self.pending.insert(local, ChunkCmd::Load);
    }

    pub fn unload(&mut self, local: IVec3) {
        if let Some(cmd) = self.running.get(&local) {
            warn!("Chunk {:?} cmd already exists: ", cmd);
            return;
        }
        self.pending.insert(local, ChunkCmd::Unload);
    }

    pub fn update(&mut self, local: IVec3, voxels: Vec<(IVec3, voxel::Kind)>) {
        self.pending.insert(local, ChunkCmd::Update(voxels));
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ChunkCmd {
    Load,
    Unload,
    Update(Vec<(IVec3, voxel::Kind)>),
}

#[derive(Clone, Debug, PartialEq)]
enum ChunkCmdResult {
    Loaded(IVec3),
    Unloaded(IVec3),
    Updated(IVec3),
}

#[derive(Default)]
struct ProcessBatchSystemMeta {
    task: Option<Task<(VoxWorld, Vec<ChunkCmdResult>)>>,
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
    // Only process batches if there is no task already running
    if let Some(ref mut task) = meta.task {
        perf_scope!(_perf);

        if let Some((world, commands)) = future::block_on(future::poll_once(task)) {
            for cmd in commands {
                match cmd {
                    ChunkCmdResult::Loaded(local) => loaded_writer.send(EvtChunkLoaded(local)),
                    ChunkCmdResult::Unloaded(local) => {
                        unloaded_writer.send(EvtChunkUnloaded(local))
                    }
                    ChunkCmdResult::Updated(local) => updated_writer.send(EvtChunkUpdated(local)),
                }
            }
            meta.task = None;
            world_res.set(world);
            batch_res.finished();
        }
    } else if !batch_res.is_empty() {
        perf_scope!(_perf);
        let batch = batch_res.take();
        let world = world_res.take();

        meta.task = Some(task_pool.spawn(async move { process_batch(world, batch) }));
    }

    assert_ne!(
        meta.task.is_none(),
        !world_res.is_ready(),
        "The world should exists only in one place at a time"
    );
    assert_ne!(
        meta.task.is_some(),
        world_res.is_ready(),
        "The world should exists only in one place at a time"
    );
}

type VoxelUpdateList = Vec<(IVec3, voxel::Kind)>;

fn split_commands(
    commands: HashMap<IVec3, ChunkCmd>,
) -> (Vec<IVec3>, Vec<IVec3>, Vec<(IVec3, VoxelUpdateList)>) {
    let mut load = vec![];
    let mut unload = vec![];
    let mut update = vec![];

    commands.into_iter().for_each(|(local, cmd)| match cmd {
        ChunkCmd::Load => load.push(local),
        ChunkCmd::Unload => unload.push(local),
        ChunkCmd::Update(v) => update.push((local, v)),
    });

    (load, unload, update)
}

fn process_batch(
    mut world: VoxWorld,
    commands: HashMap<IVec3, ChunkCmd>,
) -> (VoxWorld, Vec<ChunkCmdResult>) {
    let mut _perf = perf_fn!();

    let (load, unload, update) = split_commands(commands);

    let mut dirty_chunks = HashSet::default();

    dirty_chunks.extend(unload_chunks(&unload, &mut world));
    dirty_chunks.extend(load_chunks(&load, &mut world));
    dirty_chunks.extend(update_chunks(&update, &mut world));

    let updated_chunks = process_chunk_pipeline(dirty_chunks, &mut world);

    // TODO: Send all batch related events

    let mut result = vec![];
    result.extend(load.iter().map(|&l| ChunkCmdResult::Loaded(l)));
    result.extend(unload.iter().map(|&l| ChunkCmdResult::Unloaded(l)));
    result.extend(updated_chunks.iter().map(|&l| ChunkCmdResult::Updated(l)));

    (world, result)
}

fn update_chunks(locals: &[(IVec3, VoxelUpdateList)], world: &mut VoxWorld) -> HashSet<IVec3> {
    let mut dirty_chunks = HashSet::default();

    for (local, voxels) in locals {
        trace!("Updating chunk {} values {:?}", local, voxels);

        if let Some(chunk) = world.get_mut(*local) {
            for (voxel, kind) in voxels {
                chunk.kind.set(*voxel, *kind);

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
            warn!("Failed to set voxel. Chunk {} wasn't found.", *local);
        }
    }

    dirty_chunks
}

fn unload_chunks(locals: &[IVec3], world: &mut VoxWorld) -> HashSet<IVec3> {
    perf_fn_scope!();

    let mut dirty_chunks = HashSet::default();

    for local in locals {
        if world.remove(*local).is_none() {
            warn!("Trying to unload non-existing chunk {}", local);
        } else {
            dirty_chunks.extend(voxel::SIDES.map(|s| s.dir() + *local))
        }
    }

    dirty_chunks
}

fn load_chunks(locals: &[IVec3], world: &mut VoxWorld) -> HashSet<IVec3> {
    perf_fn_scope!();

    let mut dirty_chunks = HashSet::default();

    for local in locals {
        let path = local_path(*local);

        let chunk = if path.exists() {
            load_cache(&path)
        } else {
            generate_chunk(*local)
        };

        world.add(*local, chunk);

        dirty_chunks.extend(
            voxel::SIDES
                .iter()
                .map(|s| s.dir() + *local)
                .chain(std::iter::once(*local)),
        );
    }

    dirty_chunks
}

fn process_chunk_pipeline(chunks: HashSet<IVec3>, world: &mut VoxWorld) -> Vec<IVec3> {
    perf_fn_scope!();

    chunks
        .into_iter()
        .filter_map(|local| {
            update_neighborhood(world, local);
            world.get_mut(local).map(|chunk| {
                let occlusion = faces_occlusion(&chunk.kind);
                let faces = faces_merging(&chunk.kind, &occlusion);
                chunk.vertices = vertices_computation(faces);
                local
            })
        })
        .collect()
}

fn vertices_computation(faces: Vec<VoxelFace>) -> Vec<VoxelVertex> {
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

fn faces_merging(chunk: &ChunkKind, occlusion: &ChunkFacesOcclusion) -> Vec<VoxelFace> {
    perf_fn_scope!();

    mesh::merge_faces(occlusion, chunk)
}

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

fn update_neighborhood(world: &mut VoxWorld, local: IVec3) {
    let mut neighborhood = ChunkNeighborhood::default();
    for side in voxel::SIDES {
        let dir = side.dir();
        let neighbor = local + dir;

        if let Some(neighbor_chunk) = world.get(neighbor) {
            neighborhood.set(side, &neighbor_chunk.kind);
        }
    }

    if let Some(chunk) = world.get_mut(local) {
        chunk.kind.neighborhood = neighborhood;
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
    for x in 0..chunk::AXIS_SIZE {
        for z in 0..chunk::AXIS_SIZE {
            let h = noise.get_noise(world.x + x as f32, world.z + z as f32);
            let world_height = ((h + 1.0) / 2.0) * (2 * chunk::AXIS_SIZE) as f32;

            let height_local = world_height - world.y;

            if height_local < f32::EPSILON {
                continue;
            }

            let end = usize::min(height_local as usize, chunk::AXIS_SIZE);

            for y in 0..end {
                kinds.set((x as i32, y as i32, z as i32).into(), 1.into());
            }
        }
    }
    let path = local_path(local);

    assert!(!path.exists(), "Cache already exists!");

    let chunk = Chunk {
        kind: kinds,
        vertices: vec![],
    };
    save_chunk(&path, &chunk);

    chunk
}

fn save_chunk(path: &Path, chunk: &Chunk) {
    perf_fn_scope!();

    let file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to write to file {}", path.display()));

    bincode::serialize_into(file, chunk)
        .unwrap_or_else(|_| panic!("Failed to serialize cache to file {}", path.display()));
}

fn load_cache(path: &Path) -> Chunk {
    perf_fn_scope!();

    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to open file {}", path.display()));

    bincode::deserialize_from(file)
        .unwrap_or_else(|_| panic!("Failed to parse file {}", path.display()))
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
            ((0, chunk::AXIS_ENDING as i32, 5).into(), 3.into()),
        ];

        let dirty_chunks = super::update_voxel(&mut world, local, &voxels);

        let chunk = world.get(local).unwrap();

        assert_eq!(chunk.get((0, 0, 0).into()), 1.into());
        assert_eq!(chunk.get((1, 1, 1).into()), 2.into());
        assert_eq!(
            chunk.get((0, chunk::AXIS_ENDING as i32, 5).into()),
            3.into()
        );

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
        let cache = super::generate_chunk(local);
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

        super::save_chunk(&path, &cache);

        assert!(path.exists());

        let loaded_cache = super::load_cache(&path);

        assert_eq!(cache, loaded_cache);

        remove_file(path).unwrap();
    }
}
