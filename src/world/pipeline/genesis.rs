use std::path::PathBuf;

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
    utils::HashMap,
};
use bracket_noise::prelude::{FastNoise, FractalType, NoiseType};
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
        app.add_event::<CmdChunkLoad>()
            .add_event::<CmdChunkUnload>()
            .add_event::<CmdChunkGen>()
            .add_event::<EvtChunkLoaded>()
            .add_event::<EvtChunkUnloaded>()
            .add_startup_system_to_stage(super::PipelineStartup::Genesis, setup_vox_world)
            .add_system_to_stage(super::Pipeline::Genesis, load_cache_system.label("load"))
            .add_system_to_stage(super::Pipeline::Genesis, unload_cache_system.after("load"))
            .add_system_to_stage(super::Pipeline::Genesis, gen_cache_system);
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

pub struct CmdChunkLoad(pub IVec3);
pub struct EvtChunkLoaded(pub IVec3);

pub struct CmdChunkUnload(pub IVec3);
pub struct EvtChunkUnloaded(pub IVec3);

struct CmdChunkGen(IVec3);

fn setup_vox_world(mut commands: Commands) {
    trace_system_run!();

    commands.insert_resource(VoxWorld::default());
}

fn unload_cache_system(
    mut vox_world: ResMut<VoxWorld>,
    mut reader: EventReader<CmdChunkUnload>,
    mut writer: EventWriter<EvtChunkUnloaded>,
) {
    let mut _perf = perf_fn!();

    for CmdChunkUnload(local) in reader.iter() {
        if vox_world.remove(*local).is_none() {
            warn!("Trying to unload non-existing cache {}", *local);
        }
        writer.send(EvtChunkUnloaded(*local));
    }
}

#[derive(Default)]
struct GenCacheMeta {
    tasks: HashMap<IVec3, Task<()>>,
}

fn gen_cache_system(
    mut reader: EventReader<CmdChunkGen>,
    mut writer: EventWriter<CmdChunkLoad>,
    task_pool: Res<AsyncComputeTaskPool>,
    mut meta: Local<GenCacheMeta>,
) {
    let mut _perf = perf_fn!();

    for CmdChunkGen(local) in reader.iter() {
        trace_system_run!(local);
        perf_scope!(_perf);

        let chunk_local = *local;

        meta.tasks.insert(
            *local,
            task_pool.spawn(async move {
                let mut noise = FastNoise::seeded(15);
                noise.set_noise_type(NoiseType::SimplexFractal);
                noise.set_frequency(0.03);
                noise.set_fractal_type(FractalType::FBM);
                noise.set_fractal_octaves(3);
                noise.set_fractal_gain(0.9);
                noise.set_fractal_lacunarity(0.5);

                let world = chunk::to_world(chunk_local);
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

                let path = local_path(chunk_local);
                save_cache(
                    &path,
                    &ChunkCache {
                        local: chunk_local,
                        kind: kinds,
                    },
                );
            }),
        );
    }

    let completed_tasks = meta
        .tasks
        .iter_mut()
        .filter_map(|(&local, task)| {
            use futures_lite::future;
            future::block_on(future::poll_once(task)).map(|_| {
                writer.send(CmdChunkLoad(local));
                local
            })
        })
        .collect::<Vec<_>>();

    completed_tasks.iter().for_each(|v| {
        meta.tasks
            .remove(v)
            .expect("Task for load cache must exists");
    });
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

#[derive(Default)]
struct LoadCacheMeta {
    tasks: HashMap<IVec3, Task<ChunkCache>>,
}

fn load_cache_system(
    mut vox_world: ResMut<VoxWorld>,
    mut reader: EventReader<CmdChunkLoad>,
    mut gen_writer: EventWriter<CmdChunkGen>,
    mut added_writer: EventWriter<EvtChunkLoaded>,
    task_pool: Res<AsyncComputeTaskPool>,
    mut meta: Local<LoadCacheMeta>,
) {
    let mut _perf = perf_fn!();
    for CmdChunkLoad(local) in reader.iter() {
        trace_system_run!(local);
        perf_scope!(_perf);

        let path = local_path(*local);

        if path.exists() {
            meta.tasks
                .insert(*local, task_pool.spawn(async move { load_cache(&path) }));
        } else {
            gen_writer.send(CmdChunkGen(*local));
        }
    }

    let completed_tasks = meta
        .tasks
        .iter_mut()
        .filter_map(|(&local, task)| {
            use futures_lite::future;
            future::block_on(future::poll_once(task)).map(|cache| {
                vox_world.add(local, cache.kind);
                added_writer.send(EvtChunkLoaded(local));
                local
            })
        })
        .collect::<Vec<_>>();

    completed_tasks.iter().for_each(|v| {
        meta.tasks
            .remove(v)
            .expect("Task for load cache must exists");
    });
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
