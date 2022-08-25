use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy_log::{trace, warn};
use bevy_math::IVec3;
use bevy_tasks::{IoTaskPool, Task};
use bevy_utils::HashSet;

use itertools::Itertools;
use projekto_core::{chunk::Chunk, voxel, VoxWorld};
use projekto_shaping as shaping;

use super::ChunkCmd;

pub(super) struct TaskResult {
    pub world: VoxWorld,
    pub loaded: Vec<IVec3>,
    pub unloaded: Vec<IVec3>,
    pub updated: Vec<IVec3>,
}

/// Process a batch a list of [`ChunkCmd`]. This function takes ownership of [`VoxWorld`] since it needs to do modification on world.
///
/// This function triggers [`recompute_chunks`] whenever a new chunk is generated or is updated.
///
/// ***Returns*** the [`VoxWorld`] ownership and a list of updated chunks.
pub(super) async fn process_batch(mut world: VoxWorld, commands: Vec<ChunkCmd>) -> TaskResult {
    let (load, unload, update) = split_commands(commands);

    trace!(
        "Processing batch - Load: {}, Unload: {}, Update: {}",
        load.len(),
        unload.len(),
        update.len()
    );

    unload_chunks(&mut world, &unload);

    // The loading may take a while, so do in another task.
    let (not_found, load_task) = load_chunks(&load);

    let new_chunks = generate_chunks(not_found)
        .await
        .into_iter()
        .map(|(local, chunk)| {
            world.add(local, chunk);
            local
        })
        .collect_vec();

    if let Some(tasks) = load_task {
        for task in tasks {
            task.await
                .into_iter()
                .for_each(|(local, chunk)| world.add(local, chunk));
        }
    }

    // Get all chunks surrounding newly created chunks, so they can be refreshed
    let dirty = new_chunks
        .iter()
        .flat_map(|local| voxel::SIDES.iter().map(move |s| s.dir() + *local))
        .filter(|local| new_chunks.contains(local) == false)
        .filter(|local| world.exists(*local))
        .unique()
        .collect_vec();

    trace!("Generation completed! {} chunks dirty.", dirty.len());

    let mut gen_vertices_list = if dirty.len() == 0 {
        vec![]
    } else {
        shaping::update_neighborhood(&mut world, &dirty)
    };

    gen_vertices_list.extend(shaping::update_chunks(&mut world, &update));
    gen_vertices_list.extend(new_chunks);

    // Compute chunk vertices
    let locals = gen_vertices_list.into_iter().unique().collect_vec();
    shaping::generate_chunk_vertices(&world, &locals)
        .into_iter()
        .for_each(|(local, vertices)| {
            world
                .get_mut(local)
                .expect("Chunk should exists on vertex generation")
                .vertices = vertices
        });

    let world = if locals.len() > 0 {
        save_chunks(world, &locals).await
    } else {
        world
    };

    TaskResult {
        world,
        loaded: load,
        unloaded: unload,
        updated: locals.into_iter().collect(),
    }
}

/// Generate new chunks on given locals.
///
/// This function will do its best to calculate the values and propagation between the newly created chunks.
///
/// ***Returns*** a list of newly created chunks and their locals
async fn generate_chunks(locals: Vec<IVec3>) -> Vec<(IVec3, Chunk)> {
    if locals.is_empty() {
        return vec![];
    }

    trace!("Generating {} chunks.", locals.len());

    let new_chunks = locals
        .iter()
        .map(|&local| (local, shaping::generate_chunk(local)))
        .collect_vec();

    shaping::build_chunk_internals(new_chunks).await
}

/// Remove from [`VoxWorld`] all chunks on the given list.
///
/// ***Returns*** A list of chunks locals that are dirty due to neighboring chunks removal.
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

/// Spawn a task on [`IoTaskPool`] which will load all existing chunks.
///
/// Chunks that doesn't exists on cache (cache miss) will be returned.
///
/// ***Returns*** A list of chunks locals which doesn't exists on cache and an optional task running on [`IoTaskPool`] loading chunks.
fn load_chunks(locals: &Vec<IVec3>) -> (Vec<IVec3>, Option<Vec<Task<Vec<(IVec3, Chunk)>>>>) {
    let (exists, not_exists): (Vec<_>, Vec<_>) = locals
        .iter()
        .map(|v| (v, local_path(v)))
        .map(|(v, path)| {
            if path.exists() {
                (Some(v), None)
            } else {
                (None, Some(v))
            }
        })
        .unzip();

    let to_load = exists.into_iter().filter_map(|o| o).copied().collect_vec();
    let load_task = if to_load.is_empty() {
        None
    } else {
        let mut tasks = vec![];

        for locals in split_locals_by_cores(&to_load) {
            let task = IoTaskPool::get().spawn(async move {
                locals
                    .into_iter()
                    .map(|local| (local, load_chunk(&local_path(&local))))
                    .collect_vec()
            });

            tasks.push(task);
        }

        Some(tasks)
    };

    let not_found = not_exists
        .into_iter()
        .filter_map(|o| o)
        .copied()
        .collect_vec();

    (not_found, load_task)
}

///
///
///
async fn save_chunks(world: VoxWorld, locals: &[IVec3]) -> VoxWorld {
    trace!("Saving {} chunks on disk", locals.len(),);

    let mut tasks = vec![];

    let arc_world = Arc::new(world);

    for locals in split_locals_by_cores(locals) {
        let wd = arc_world.clone();
        let task = IoTaskPool::get().spawn(async move {
            for local in locals {
                save_chunk(&local_path(&local), wd.get(local).unwrap());
            }
        });
        tasks.push(task);
    }

    for task in tasks {
        task.await;
    }

    trace!("Save completed!");

    Arc::try_unwrap(arc_world).expect("There should be no tasks running at this point")
}

fn split_locals_by_cores(locals: &[IVec3]) -> Vec<Vec<IVec3>> {
    let parallel_tasks = IoTaskPool::get().thread_num();
    let chunk_split = usize::clamp(locals.len() / parallel_tasks, 1, locals.len());

    locals
        .iter()
        .copied()
        .chunks(chunk_split)
        .into_iter()
        .map(|c| c.collect_vec())
        .collect_vec()
}

/**
 Saves the given [`Chunk`] on disk at [`Path`].
*/
fn save_chunk(path: &Path, chunk: &Chunk) {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to write to file {}", path.display()));

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

fn load_chunk(path: &Path) -> Chunk {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Unable to open file {}", path.display()));

    let mut compressed = Vec::new();
    file.read_to_end(&mut compressed)
        .unwrap_or_else(|_| panic!("Failed to read file {}", path.display()));

    let decompressed = lz4_flex::decompress_size_prepended(&compressed)
        .unwrap_or_else(|_| panic!("Failed to decompress cache {}", path.display()));

    let chunk = bincode::deserialize(&decompressed)
        .unwrap_or_else(|_| panic!("Failed to parse file {}", path.display()));

    chunk
}

fn local_path(local: &IVec3) -> PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir()
            .with_file_name(format_local(local))
            .with_extension(super::CACHE_EXT)
    }

    #[cfg(not(test))]
    {
        PathBuf::from(super::CACHE_PATH)
            .with_file_name(format_local(local))
            .with_extension(super::CACHE_EXT)
    }
}

fn format_local(local: &IVec3) -> String {
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

/**
Utility function that splits the given list of [`ChunkCmd`] into individual cmd lists

***Returns*** tuple with load, unload and update cmd lists
 */
fn split_commands(
    commands: Vec<ChunkCmd>,
) -> (
    Vec<IVec3>,
    Vec<IVec3>,
    Vec<(IVec3, Vec<(IVec3, voxel::Kind)>)>,
) {
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

#[cfg(test)]
mod tests {
    use std::fs::remove_file;

    use futures_lite::future::block_on;

    use super::*;

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
        IoTaskPool::init(|| Default::default());

        // Load existing cache
        let local = (9943, 9943, 9999).into();
        let path = super::local_path(&local);
        let chunk = Chunk::default();

        create_chunk_on_disk(&path, &chunk);

        let (dirty_chunks, tasks) = super::load_chunks(&vec![local]);

        assert_eq!(
            dirty_chunks.len(),
            0,
            "The chunk already exists, so no dirty chunks"
        );

        assert!(tasks.is_some(), "A valid task should be returned");

        let task = tasks.unwrap().remove(0);
        let chunks = block_on(task);
        assert!(
            chunks.iter().find(|(l, _)| *l == local).is_some(),
            "Chunk should be added to world"
        );

        let _ = remove_file(path);

        // Load non-existing cache
        let local = (9942, 9944, 9421).into();

        let (not_loaded, task) = super::load_chunks(&vec![local]);

        assert_eq!(
            not_loaded.len(),
            1,
            "Chunk doesn't exists, so it must be reported as not loaded"
        );

        assert!(
            task.is_none(),
            "No task should be spawned, since there is no chunk to be loaded"
        );

        assert!(not_loaded.contains(&local));
    }

    #[test]
    fn local_path_test() {
        let path = super::local_path(&(0, 0, 0).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("0_0_0.{}", super::super::CACHE_EXT)));

        let path = super::local_path(&(-1, 0, 0).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("-1_0_0.{}", super::super::CACHE_EXT)));

        let path = super::local_path(&(-1, 3333, -461).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("-1_3333_-461.{}", super::super::CACHE_EXT)));
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

        create_chunk_on_disk(&temp_file, &chunk);

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .open(&temp_file)
            .unwrap();

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

    fn create_chunk_on_disk(path: &Path, chunk: &Chunk) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();

        let uncompressed = bincode::serialize(chunk).unwrap();
        let compressed = lz4_flex::compress_prepend_size(&uncompressed);
        file.write_all(&compressed).unwrap();
    }

    #[test]
    fn format_local() {
        assert_eq!("-234_22_1", super::format_local(&(-234, 22, 1).into()));
        assert_eq!(
            "-9999_-9999_-9999",
            super::format_local(&(-9999, -9999, -9999).into())
        );
        assert_eq!(
            "9999_-9999_9999",
            super::format_local(&(9999, -9999, 9999).into())
        );
        assert_eq!("0_0_0", super::format_local(&(0, 0, 0).into()));
    }

    #[test]
    fn load_cache() {
        let local = (-9998, 0, 9998).into();

        let chunk = Default::default();

        let path = local_path(&local);
        create_chunk_on_disk(&path, &chunk);

        let loaded_chunk = super::load_chunk(&path);

        assert_eq!(chunk, loaded_chunk);

        remove_file(path).unwrap();
    }

    #[test]
    fn save_cache() {
        let local = (-921, 0, 2319).into();

        let chunk = Default::default();

        let path = local_path(&local);

        assert!(!path.exists());

        super::save_chunk(&path, &chunk);

        assert!(path.exists());

        let loaded_cache = super::load_chunk(&path);

        assert_eq!(chunk, loaded_cache);

        remove_file(path).unwrap();
    }
}
