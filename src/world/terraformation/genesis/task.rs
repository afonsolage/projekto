use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

use bevy::{
    prelude::{trace, warn, IVec3},
    tasks::IoTaskPool,
    utils::{HashMap, HashSet},
};
use itertools::Itertools;
use projekto_core::{
    chunk::{self, Chunk},
    voxel, VoxWorld,
};
use projekto_shaping as shaping;

use crate::world::terraformation::VoxelUpdateList;

use super::ChunkCmd;

pub(super) struct TaskResult {
    pub world: VoxWorld,
    pub loaded: Vec<IVec3>,
    pub unloaded: Vec<IVec3>,
    pub updated: Vec<IVec3>,
}

/**
Process a batch a list of [`ChunkCmd`]. This function takes ownership of [`VoxWorld`] since it needs to do modification on world.

This function triggers [`recompute_chunks`] whenever a new chunk is generated or is updated.

***Returns*** the [`VoxWorld`] ownership and a list of updated chunks.
 */
pub(super) async fn process_batch(mut world: VoxWorld, commands: Vec<ChunkCmd>) -> TaskResult {
    let mut _perf = perf_fn!();

    let (load, unload, update) = split_commands(commands);

    trace!(
        "Processing batch - Load: {}, Unload: {}, Update: {}",
        load.len(),
        unload.len(),
        update.len()
    );

    unload_chunks(&mut world, &unload);

    let not_found = load_chunks(&mut world, load.clone()).await;

    let mut updated = generate_chunks(&mut world, not_found)
        .into_iter()
        .collect::<HashSet<_>>();

    trace!("Generation completed! {} chunks updated.", updated.len());

    updated.extend(update_chunks(&mut world, &update));

    // let dirty_chunks = dirty_chunks.into_iter().collect::<Vec<_>>();
    // let updated = recompute_chunks(&mut world, kinds_descs, dirty_chunks);

    TaskResult {
        world,
        loaded: load,
        unloaded: unload,
        updated: updated.into_iter().collect(),
    }
}

/**
Applies on the given [`VoxWorld`] a voxel modification list [`VoxelUpdateList`]

***Returns*** A list of chunks locals that are dirty due to voxel modifications. This is usually neighboring chunks where voxel was updated
 */
fn update_chunks(world: &mut VoxWorld, update_list: &[(IVec3, VoxelUpdateList)]) -> Vec<IVec3> {
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
    recompute_chunks_internals(world, &updated_chunks)
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
async fn load_chunks(world: &mut VoxWorld, locals: Vec<IVec3>) -> Vec<IVec3> {
    let loaded_chunks = IoTaskPool::get()
        .spawn(async move {
            locals
                .iter()
                .map(|local| {
                    let path = local_path(*local);
                    if path.exists() {
                        (*local, Some(load_chunk(&path)))
                    } else {
                        (*local, None)
                    }
                })
                .collect_vec()
        })
        .await;

    loaded_chunks
        .into_iter()
        .filter_map(|(local, chunk)| {
            if let Some(chunk) = chunk {
                world.add(local, chunk);
                None
            } else {
                Some(local)
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
    update: &[(IVec3, VoxelUpdateList)],
) -> Vec<IVec3> {
    perf_fn_scope!();

    let locals = shaping::recompute_chunks_internals(world, update);

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
fn compute_chunks_internals(world: &mut VoxWorld, locals: Vec<IVec3>) -> Vec<IVec3> {
    perf_fn_scope!();

    let locals = shaping::compute_chunks_internals(world, locals);

    trace!("Saving {} chunks on disk!", locals.len());

    // TODO: Find a way to only saving chunks which was really updated.
    for local in locals.iter() {
        if let Some(chunk) = world.get(*local) {
            let path = local_path(*local);
            save_chunk(&path, chunk);
        }
    }

    locals
}

fn generate_chunks(world: &mut VoxWorld, locals: Vec<IVec3>) -> Vec<IVec3> {
    trace!("Generating {} chunks.", locals.len());

    // Before doing anything else, all generated chunks have to be added to world.
    locals.iter().for_each(|&local| {
        world.add(local, shaping::generate_chunk(local));
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
        .unique()
        .collect();

    compute_chunks_internals(world, dirty_chunks)
}

/**
 Saves the given [`Chunk`] on disk at [`Path`].
*/
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
    PathBuf::from(super::CACHE_PATH)
        .with_file_name(format_local(local))
        .with_extension(super::CACHE_EXT)
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

#[cfg(test)]
mod tests {
    use std::fs::remove_file;

    use bevy::prelude::Vec3;
    use futures_lite::future::block_on;
    use projekto_core::voxel::{Light, LightTy};

    use super::*;

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
            vec![(0, 0, 0).into(), (1, 0, 0).into()],
        );

        let chunk = world.get((0, 0, 0).into()).unwrap();
        assert_eq!(chunk.lights.get((15, 6, 0).into()).get(LightTy::Natural), 4, "Failed to compute chunk internals. This is likely a bug handled by others tests. Ignore this and fix others.");

        let neighbor = world.get((1, 0, 0).into()).unwrap();
        assert_eq!(neighbor.lights.get((0, 6, 0).into()).get(LightTy::Natural), 5, "Failed to compute chunk internals. This is likely a bug handled by others tests. Ignore this and fix others.");

        world
    }

    #[test]
    fn update_chunks_neighbor_side_light() {
        let mut world = create_test_world();

        let update_list = [((0, 0, 0).into(), vec![((15, 10, 0).into(), 0.into())])];

        let updated = super::update_chunks(&mut world, &update_list);

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
            chunk.lights.get((15, 10, 0).into()).get(LightTy::Natural),
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
        assert_eq!(
            updated_voxel_side_vertex.light,
            Vec3::new(0.25, 0.25, 0.25),
            "Should return 1/4 or light intensity, since all neighbors are occluded"
        );
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

        let dirty_chunks = super::update_chunks(&mut world, &vec![(local, voxels)]);

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
        IoTaskPool::init(|| Default::default());

        // Load existing cache
        let local = (9943, 9943, 9999).into();
        let path = super::local_path(local);
        let chunk = Chunk::default();

        create_chunk(&path, &chunk);

        let mut world = VoxWorld::default();

        let dirty_chunks = block_on(super::load_chunks(&mut world, vec![local]));

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
        let not_loaded = block_on(super::load_chunks(&mut world, vec![local]));

        assert_eq!(
            not_loaded.len(),
            1,
            "Chunk doesn't exists, so it must be reported as not loaded"
        );
        assert!(not_loaded.contains(&local));
        assert!(!world.exists(local), "Chunk should not be added to world");
    }

    #[test]
    fn local_path_test() {
        let path = super::local_path((0, 0, 0).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("0_0_0.{}", super::super::CACHE_EXT)));

        let path = super::local_path((-1, 0, 0).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("-1_0_0.{}", super::super::CACHE_EXT)));

        let path = super::local_path((-1, 3333, -461).into())
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
}
