use std::{ops::Deref, path::PathBuf};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
};
use bracket_noise::prelude::{FastNoise, FractalType, NoiseType};
use futures_lite::future;
use serde::{Deserialize, Serialize};

use crate::world::storage::{
    chunk::{self, ChunkKind},
    VoxWorld,
};

const CACHE_PATH: &'static str = "cache/chunks/example";
const CACHE_EXT: &'static str = "ron";

pub(super) struct GenesisPlugin;

impl Plugin for GenesisPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkLoaded>()
            .add_event::<EvtChunkUnloaded>()
            .add_event::<EvtChunkUpdated>()
            .add_startup_system_to_stage(super::PipelineStartup::Genesis, setup_resources)
            .add_system_to_stage(super::Pipeline::Genesis, update_world_system);
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

    commands.insert_resource(WorldRes(Some(VoxWorld::default())));
    commands.insert_resource(BatchChunkCmdRes::default());
}

#[derive(Default)]
pub struct BatchChunkCmdRes {
    pending: Vec<ChunkCmd>,
    running: Vec<ChunkCmd>,
}

impl BatchChunkCmdRes {
    fn take(&mut self) -> Vec<ChunkCmd> {
        self.running = std::mem::replace(&mut self.pending, vec![]);
        self.running.clone()
    }

    fn finished(&mut self) {
        self.running.clear();
    }

    fn is_cmd_running(&self, cmd: ChunkCmd) -> bool {
        self.running.iter().any(|running_cmd| running_cmd == &cmd)
    }

    fn remove_pending_cmd(&mut self, local: IVec3) {
        self.pending.retain(|cmd| {
            let i = match cmd {
                ChunkCmd::Load(i) => i,
                ChunkCmd::Unload(i) => i,
                ChunkCmd::Update(i) => i,
            };
            *i != local
        });
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn load(&mut self, local: IVec3) {
        let cmd = ChunkCmd::Load(local);
        if self.is_cmd_running(cmd) {
            warn!("Chunk {} is already loading", local);
            return;
        }

        self.remove_pending_cmd(local);
        self.pending.push(cmd);
    }

    pub fn unload(&mut self, local: IVec3) {
        let cmd = ChunkCmd::Unload(local);
        if self.is_cmd_running(cmd) {
            warn!("Chunk {} is already unloading", local);
            return;
        }

        self.remove_pending_cmd(local);
        self.pending.push(cmd);
    }

    pub fn update(&mut self, local: IVec3) {
        let cmd = ChunkCmd::Update(local);
        if self.is_cmd_running(cmd) {
            warn!("Chunk {} is already updating", local);
            return;
        }

        self.remove_pending_cmd(local);
        self.pending.push(cmd);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ChunkCmd {
    Load(IVec3),
    Unload(IVec3),
    Update(IVec3),
}

#[derive(Default)]
struct ProcessBatchSystemMeta {
    task: Option<Task<(VoxWorld, Vec<ChunkCmd>)>>,
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
    // Only process batches if there is no task already running
    if let Some(ref mut task) = meta.task {
        if let Some((world, commands)) = future::block_on(future::poll_once(task)) {
            for cmd in commands {
                match cmd {
                    ChunkCmd::Load(local) => loaded_writer.send(EvtChunkLoaded(local)),
                    ChunkCmd::Unload(local) => unloaded_writer.send(EvtChunkUnloaded(local)),
                    ChunkCmd::Update(local) => updated_writer.send(EvtChunkUpdated(local)),
                }
            }
            meta.task = None;
            world_res.set(world);
            batch_res.finished();
        }
    } else if !batch_res.is_empty() {
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

fn process_batch(mut world: VoxWorld, commands: Vec<ChunkCmd>) -> (VoxWorld, Vec<ChunkCmd>) {
    for local in commands.iter().filter_map(|cmd| match cmd {
        ChunkCmd::Unload(i) => Some(*i),
        _ => None,
    }) {
        unload_chunk(&mut world, local);
    }

    for local in commands.iter().filter_map(|cmd| match cmd {
        ChunkCmd::Load(i) => Some(*i),
        _ => None,
    }) {
        load_chunk(&mut world, local);
    }

    for local in commands.iter().filter_map(|cmd| match cmd {
        ChunkCmd::Load(i) | ChunkCmd::Update(i) => Some(*i),
        _ => None,
    }) {
        update_chunk(&mut world, local);
    }

    (world, commands)
}

fn unload_chunk(world: &mut VoxWorld, local: IVec3) {
    if world.remove(local).is_none() {
        warn!("Trying to unload non-existing cache {}", local);
    }
}

fn load_chunk(world: &mut VoxWorld, local: IVec3) {
    let path = local_path(local);

    let cache = if path.exists() {
        load_cache(&path)
    } else {
        generate_cache(local)
    };

    world.add(local, cache.kind);
}

fn update_chunk(world: &mut VoxWorld, local: IVec3) {
    world.update_neighborhood(local);
}

fn generate_cache(local: IVec3) -> ChunkCache {
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
    let chunk_cache = ChunkCache { local, kind: kinds };
    save_cache(&path, &chunk_cache);

    chunk_cache
}

fn save_cache(path: &PathBuf, cache: &ChunkCache) {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .expect(&format!("Unable to write to file {}", path.display()));

    ron::ser::to_writer(file, cache).expect(&format!(
        "Failed to serialize cache to file {}",
        path.display()
    ));
}

fn load_cache(path: &PathBuf) -> ChunkCache {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .expect(&format!("Unable to open file {}", path.display()));

    ron::de::from_reader(file).expect(&format!("Failed to parse file {}", path.display()))
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

        let cache_loaded: ChunkCache = ron::de::from_reader(file).unwrap();

        assert_eq!(cache, cache_loaded);
    }

    fn create_cache(path: &PathBuf, cache: &ChunkCache) {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();
        ron::ser::to_writer(file, cache).unwrap();
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
}
