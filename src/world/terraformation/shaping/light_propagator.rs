use std::collections::VecDeque;

use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};

use crate::world::{
    storage::{
        chunk::{self, Chunk, ChunkNeighborhood},
        voxel::{self, LightTy},
        VoxWorld,
    },
    terraformation::VoxelUpdateList,
};

fn update_chunk_light_neighborhood(world: &mut VoxWorld, local: IVec3) {
    perf_fn_scope!();

    let mut neighborhood = ChunkNeighborhood::default();
    for side in voxel::SIDES {
        let dir = side.dir();
        let neighbor = local + dir;

        if let Some(neighbor_chunk) = world.get(neighbor) {
            neighborhood.set(side, &neighbor_chunk.lights);
        }
    }

    let chunk = world.get_mut(local).unwrap();
    chunk.lights.neighborhood = neighborhood;
}

pub fn propagate_natural_light_on_new_chunks(world: &mut VoxWorld, locals: &[IVec3]) {
    trace!("Propagating natural light on new {} chunks", locals.len());

    // Propagate initial top-down natural light across all given chunks.
    // This function propagate the natural light from the top-most voxels downwards.
    // This function only propagate internally, does not spread the light to neighbors.
    propagate_natural_light_top_down(world, locals);

    // Propagates natural light from/into the neighborhood
    // This function must be called after natural internal propagation, since it will check neighbors only
    propagate_natural_light_neighborhood(world, locals);
}

fn propagate_natural_light_neighborhood(world: &mut VoxWorld, locals: &[IVec3]) -> Vec<IVec3> {
    perf_fn_scope!();

    trace!(
        "Preparing to propagate natural light to neighbors of {} chunks",
        locals.len()
    );

    // Map all voxels on the edge of chunk and propagate it's light to the neighborhood.
    let chunks_boundary_voxels = locals
        .iter()
        .map(|&local| {
            (
                local,
                voxel::SIDES
                    .iter()
                    .flat_map(|&side| ChunkNeighborhood::<voxel::Light>::side_iterator(side))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();

    trace!(
        "Propagating light on {} chunk neighbors",
        chunks_boundary_voxels.keys().len()
    );

    propagate_natural_light(world, chunks_boundary_voxels)
}

fn propagate_natural_light_top_down(world: &mut VoxWorld, locals: &[IVec3]) {
    perf_fn_scope!();

    let top_voxels = (0..=chunk::X_END)
        .flat_map(|x| (0..=chunk::Z_END).map(move |z| (x, chunk::Y_END, z).into()))
        .collect::<Vec<_>>();

    for &local in locals {
        let chunk = world.get_mut(local).unwrap();

        propagate_chunk_natural_light(chunk, &top_voxels, false);
    }
}

fn find_highest_surrounding_light(
    world: &mut VoxWorld,
    local: IVec3,
    voxel: IVec3,
) -> Option<(IVec3, IVec3)> {
    let chunk = world.get(local)?;

    // Get side with highest intensity
    let (highest_side, intensity) = voxel::SIDES
        .iter()
        .filter_map(|&side| {
            chunk
                .lights
                .get_absolute(voxel + side.dir())
                .map(|l| (side, l.get(LightTy::Natural)))
        })
        .max_by_key(|&(_, l)| l)?;

    if intensity <= 1 {
        None
    } else {
        let side_voxel = voxel + highest_side.dir();

        if chunk::is_within_bounds(side_voxel) {
            Some((local, side_voxel))
        } else {
            let (_, neighbor_voxel) = chunk::overlap_voxel(side_voxel);
            Some((local + highest_side.dir(), neighbor_voxel))
        }
    }
}

pub fn update_light(world: &mut VoxWorld, updated: &[(IVec3, VoxelUpdateList)]) -> Vec<IVec3> {
    perf_fn_scope!();

    let (mut removal, mut propagation) = (HashMap::new(), HashMap::new());

    // Split updated list in removal and propagation
    for (local, voxels_update) in updated {
        for (voxel, new_kind) in voxels_update {
            if new_kind.is_opaque() {
                removal.entry(*local).or_insert(vec![]).push(*voxel);
            } else {
                // Get the highest surrounding light source and propagate to current voxel
                if let Some((propagation_source_local, propagation_source_voxel)) =
                    find_highest_surrounding_light(world, *local, *voxel)
                {
                    propagation
                        .entry(propagation_source_local)
                        .or_insert(vec![])
                        .push(propagation_source_voxel);
                }
            };
        }
    }

    let mut touched = vec![];

    // TODO: Implement emission

    touched.extend(remove_natural_light(world, removal.into_iter().collect()));
    touched.extend(propagate_natural_light(
        world,
        propagation.into_iter().collect(),
    ));

    for &local in touched.iter() {
        if world.exists(local) {
            update_chunk_light_neighborhood(world, local);
        }
    }

    touched
}

pub type ChunkVoxelMap = HashMap<IVec3, Vec<IVec3>>;

fn propagate_natural_light(world: &mut VoxWorld, voxels: ChunkVoxelMap) -> Vec<IVec3> {
    perf_fn_scope!();

    let mut dirty_chunks = HashSet::new();

    let mut propagate_queue = voxels.into_iter().collect::<VecDeque<_>>();

    while let Some((local, voxels)) = propagate_queue.pop_front() {
        if !world.exists(local) {
            continue;
        }

        // TODO: Check if it's possible to optimize this later on
        update_chunk_light_neighborhood(world, local);

        // Apply propagation on current chunk, if exists, and get a list of propagations to be applied on neighbors.
        let neighbor_propagation =
            propagate_chunk_natural_light(world.get_mut(local).unwrap(), &voxels, true);

        // Flag current chunk and all propagated neighbors as dirty.
        dirty_chunks.insert(local);

        for (neighbor_dir, neighbor_lights) in neighbor_propagation {
            let neighbor_local = neighbor_dir + local;
            if let Some(neighbor_chunk) = world.get_mut(neighbor_local) {
                dirty_chunks.insert(neighbor_local);

                let mut propagation_list = vec![];

                for (neighbor_voxel, new_intensity) in neighbor_lights {
                    neighbor_chunk
                        .lights
                        .set_natural(neighbor_voxel, new_intensity);

                    dirty_chunks.extend(chunk::neighboring(neighbor_local, neighbor_voxel));

                    if new_intensity > 1 {
                        propagation_list.push(neighbor_voxel);
                    }
                }

                if !propagation_list.is_empty() {
                    propagate_queue.push_back((neighbor_local, propagation_list));
                }
            }
        }
    }

    dirty_chunks.into_iter().collect()
}

fn remove_natural_light(world: &mut VoxWorld, voxels: ChunkVoxelMap) -> Vec<IVec3> {
    perf_fn_scope!();

    let mut touched_chunks = HashSet::new();

    let mut remove_queue = voxels.into_iter().collect::<VecDeque<_>>();
    let mut propagate_queue = VecDeque::<(IVec3, Vec<IVec3>)>::new();

    while let Some((local, voxels)) = remove_queue.pop_front() {
        // TODO: Check if it's possible to optimize this later on
        update_chunk_light_neighborhood(world, local);

        if let Some(chunk) = world.get_mut(local) {
            let RemoveChunkNaturalLightResult { remove, propagate } =
                remove_chunk_natural_light(chunk, &voxels);

            let removed = remove
                .into_iter()
                .map(|(dir, voxels)| (dir + local, voxels))
                .collect::<Vec<_>>();

            touched_chunks.extend(removed.iter().map(|(l, _)| *l));
            remove_queue.extend(removed);

            let propagated = propagate
                .into_iter()
                .map(|(dir, voxels)| (dir + local, voxels))
                .collect::<Vec<_>>();

            touched_chunks.extend(propagated.iter().map(|(l, _)| *l));
            propagate_queue.extend(propagated);
        }
    }

    touched_chunks.extend(propagate_natural_light(
        world,
        propagate_queue.into_iter().collect(),
    ));

    touched_chunks.into_iter().collect()
}

struct RemoveChunkNaturalLightResult {
    propagate: ChunkVoxelMap,
    remove: ChunkVoxelMap,
}

fn remove_chunk_natural_light(
    chunk: &mut Chunk,
    voxels: &[IVec3],
) -> RemoveChunkNaturalLightResult {
    perf_fn_scope!();

    // Remove all natural light from given voxels and queue'em up with older intensity value
    let mut queue = voxels
        .iter()
        .map(|&voxel| {
            let intensity = chunk.lights.get_natural(voxel);
            chunk.lights.set_natural(voxel, 0);
            (voxel, intensity)
        })
        .collect::<VecDeque<_>>();

    let mut propagate = ChunkVoxelMap::new();
    let mut remove = ChunkVoxelMap::new();

    while let Some((voxel, old_intensity)) = queue.pop_front() {
        for side in voxel::SIDES {
            let side_voxel = voxel + side.dir();

            if chunk::is_within_bounds(side_voxel) {
                let side_intensity = chunk.lights.get_natural(side_voxel);

                if (side == voxel::Side::Down
                    && side_intensity == voxel::Light::MAX_NATURAL_INTENSITY)
                    || (side_intensity != 0 && old_intensity > side_intensity)
                {
                    chunk.lights.set_natural(side_voxel, 0);

                    queue.push_back((side_voxel, side_intensity));
                } else if side_intensity >= old_intensity {
                    propagate
                        .entry((0, 0, 0).into())
                        .or_default()
                        .push(side_voxel);
                }
            } else {
                if let Some(neighbor_light) = chunk.lights.get_absolute(side_voxel) {
                    let neighbor_intensity = neighbor_light.get(voxel::LightTy::Natural);

                    let (_, neighbor_voxel) = chunk::overlap_voxel(side_voxel);

                    if neighbor_intensity != 0 && old_intensity > neighbor_intensity {
                        remove.entry(side.dir()).or_default().push(neighbor_voxel);
                    } else if neighbor_intensity >= old_intensity {
                        propagate.entry((0, 0, 0).into()).or_default().push(voxel);
                    }
                }
            }
        }
    }

    RemoveChunkNaturalLightResult { propagate, remove }
}

#[inline]
fn calc_propagated_intensity(side: voxel::Side, intensity: u8) -> u8 {
    match side {
        voxel::Side::Down if intensity == voxel::Light::MAX_NATURAL_INTENSITY => intensity,
        _ if intensity > 0 => intensity - 1,
        _ => 0,
    }
}

/// Propagates light using a flood-fill algorithm with BFS tree.
///
/// **Returns** a map with values to propagate on neighbors.
/// The map key is the chunk direction relative to current one.
fn propagate_chunk_natural_light(
    chunk: &mut Chunk,
    voxels: &[IVec3],
    propagate_to_neighbors: bool,
) -> HashMap<IVec3, Vec<(IVec3, u8)>> {
    perf_fn_scope!();

    let mut queue = voxels.iter().cloned().collect::<VecDeque<_>>();

    let mut neighbors_propagation = HashMap::new();

    while let Some(voxel) = queue.pop_front() {
        if chunk.kinds.get(voxel).is_opaque() {
            continue;
        }

        let current_intensity = chunk.lights.get_natural(voxel);

        for side in voxel::SIDES {
            let side_voxel = voxel + side.dir();

            // Skip if there is no side_voxel or if it's opaque
            if let Some(side_kind) = chunk.kinds.get_absolute(side_voxel) && !side_kind.is_opaque() {
            } else {
                continue;
            }

            let propagated_intensity = calc_propagated_intensity(side, current_intensity);

            if chunk::is_within_bounds(side_voxel) {
                // Propagate inside the chunk
                let side_intensity = chunk.lights.get_natural(side_voxel);

                if propagated_intensity > side_intensity {
                    chunk.lights.set_natural(side_voxel, propagated_intensity);

                    // TODO: Find a better way to distinguish between dirty chunks and propagation chunks
                    // If current side_voxel is on the edge, flag all neighbors as dirty, so they can be updated.
                    if chunk::is_at_bounds(side_voxel) {
                        for dir in chunk::neighboring((0, 0, 0).into(), side_voxel) {
                            let _ = neighbors_propagation.entry(dir).or_insert(vec![]);
                        }
                    }

                    if propagated_intensity > 1 {
                        queue.push_back(side_voxel);
                    }
                }
            } else if propagate_to_neighbors {
                // Propagate outside the chunk

                let (_, neighbor_voxel) = chunk::overlap_voxel(side_voxel);

                let neighbor_intensity = match chunk.lights.neighborhood.get(side, neighbor_voxel) {
                    Some(l) => l.get(LightTy::Natural),
                    None => continue,
                };

                if propagated_intensity > neighbor_intensity {
                    // Flag neighbor to propagate light on verified voxel

                    neighbors_propagation
                        .entry(side.dir())
                        .or_insert(vec![])
                        .push((neighbor_voxel, propagated_intensity));
                }
            }
        }
    }

    neighbors_propagation
}

#[cfg(test)]
mod tests {
    extern crate test;

    use test::{black_box, Bencher};

    use crate::world::storage::voxel::Light;

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

    fn fill_z_axis(z: i32, chunk: &mut Chunk) {
        for x in 0..=chunk::X_END {
            for y in 0..=chunk::Y_END {
                chunk.kinds.set((x, y, z).into(), 1.into());
            }
        }
    }

    fn build_world() -> VoxWorld {
        let mut world = VoxWorld::default();

        for x in 0..2 {
            for z in 0..2 {
                let mut chunk = Chunk::default();

                set_natural_light_on_top_voxels(&mut chunk);

                world.add((x, 0, z).into(), chunk);
            }
        }

        world
    }

    #[bench]
    fn propagate_natural_light_on_new_chunks(b: &mut Bencher) {
        let world = build_world();
        let locals = world.list_chunks();
        b.iter(|| {
            let mut cloned_world = world.clone();
            black_box(super::propagate_natural_light_on_new_chunks(
                &mut cloned_world,
                &locals,
            ));
        });
    }

    #[test]
    fn update_light_simple() {
        let mut chunk = Chunk::default();
        set_natural_light_on_top_voxels(&mut chunk);
        fill_z_axis(1, &mut chunk);

        chunk.kinds.set((1, 0, 0).into(), 1.into());
        chunk.kinds.set((1, 1, 0).into(), 1.into());
        chunk.kinds.set((1, 2, 0).into(), 1.into());

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);

        super::propagate_natural_light_on_new_chunks(&mut world, &[(0, 0, 0).into()]);

        let chunk = world.get_mut((0, 0, 0).into()).unwrap();
        assert_eq!(chunk.lights.get_natural((1, 3, 0).into()), 15);

        chunk.kinds.set((1, 2, 0).into(), 0.into());
        drop(chunk);

        super::update_light(
            &mut world,
            &[((0, 0, 0).into(), vec![((1, 2, 0).into(), 0.into())])],
        );

        let chunk = world.get_mut((0, 0, 0).into()).unwrap();
        assert_eq!(chunk.lights.get_natural((1, 2, 0).into()), 15);
    }

    #[test]
    fn propagate_chunk_natural_light_empty() {
        let mut chunk = Chunk::default();

        set_natural_light_on_top_voxels(&mut chunk);
        super::propagate_chunk_natural_light(&mut chunk, &top_voxels().collect::<Vec<_>>(), false);

        // Test the test function
        assert_eq!(
            top_voxels().count(),
            chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE
        );

        for local in chunk::voxels() {
            assert_eq!(
                chunk.lights.get(local).get(voxel::LightTy::Natural),
                Light::MAX_NATURAL_INTENSITY
            );
        }
    }

    #[test]
    fn propagate_chunk_natural_light_neighborhood() {
        /*
                           Chunk               Neighbor                    Chunk               Neighbor
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     11 | -- | 15 |        | -- | -- | 15 |          11 | -- | 15 |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     10 | -- | 15 |        | -- | -- | 15 |          10 | -- | 15 |        | 14*| -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     9  | -- | -- |        | 0  | -- | 15 |          9  | -- | -- |        | 13 | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     8  | -- | 2  |        | 1  | -- | 15 |          8  | -- | 11 |        | 12 | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     7  | -- | 3  |        | -- | -- | 15 |          7  | -- | 10 |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     6  | -- | 4  |        | 5  | -- | 15 |    ->    6  | -- | 9  |        | 8  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     5  | -- | -- |        | 6  | -- | 15 |          5  | -- | -- |        | 7  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     4  | -- | 8  |        | 7  | -- | 15 |          4  | -- | 8  |        | 7  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     3  | -- | 9  |        | -- | -- | 15 |          3  | -- | 9  |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
        Y            2  | -- | 10 |        | 11 | -- | 15 |          2  | -- | 10 |        | 11 | -- | 15 |
        |               +----+----+        +----+----+----+             +----+----+        +----+----+----+
        |            1  | -- | -- |        | 12 | -- | 15 |          1  | -- | -- |        | 12 | -- | 15 |
        + ---- X        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     0  | -- | 12 |        | 13 | 14 | 15 |          0  | -- | 12 |        | 13 | 14 | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+

                     +    14   15            0    1    2             +    14   15            0    1    2
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
        chunk.kinds.set((15, 10, 0).into(), 0.into());
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

        super::super::update_kind_neighborhoods(
            &mut world,
            vec![(0, 0, 0).into(), (1, 0, 0).into()].iter(),
        );
        super::propagate_natural_light_on_new_chunks(
            &mut world,
            &vec![(0, 0, 0).into(), (1, 0, 0).into()],
        );

        assert_eq!(
            world
                .get((1, 0, 0).into())
                .unwrap()
                .lights
                .get_natural((0, 0, 0).into()),
            13,
            "Light propagation failed. This is handled in another test"
        );

        assert_eq!(
            world
                .get((0, 0, 0).into())
                .unwrap()
                .lights
                .get_natural((15, 0, 0).into()),
            12,
            "Light propagation failed. This is handled in another test"
        );

        let neighbor = world.get_mut((1, 0, 0).into()).unwrap();
        neighbor.kinds.set((0, 10, 0).into(), 0.into());
        drop(neighbor);

        super::super::update_kind_neighborhoods(
            &mut world,
            vec![(0, 0, 0).into(), (1, 0, 0).into()].iter(),
        );

        let updated = [((1, 0, 0).into(), vec![((0, 10, 0).into(), 0.into())])];

        //Act
        super::update_light(&mut world, &updated);

        // Check neighbor
        let neighbor = world.get((1, 0, 0).into()).unwrap();
        let expected_neighbor = [
            ((0, 0, 0).into(), 13),
            ((0, 1, 0).into(), 12),
            ((0, 2, 0).into(), 11),
            ((0, 4, 0).into(), 7),
            ((0, 5, 0).into(), 7),
            ((0, 6, 0).into(), 8),
            ((0, 8, 0).into(), 12),
            ((0, 9, 0).into(), 13),
            ((0, 10, 0).into(), 14),
        ];

        for (voxel, intensity) in expected_neighbor {
            assert_eq!(
                neighbor.lights.get_natural(voxel),
                intensity,
                "Failed at {voxel}"
            );
        }

        // Check chunk
        let chunk = world.get((0, 0, 0).into()).unwrap();
        let expected_chunk = [
            ((15, 0, 0).into(), 12),
            ((15, 2, 0).into(), 10),
            ((15, 3, 0).into(), 9),
            ((15, 4, 0).into(), 8),
            ((15, 6, 0).into(), 9),
            ((15, 7, 0).into(), 10),
            ((15, 8, 0).into(), 11),
        ];

        for (voxel, intensity) in expected_chunk {
            assert_eq!(
                chunk.lights.get_natural(voxel),
                intensity,
                "Failed at {voxel}"
            );
        }
    }

    #[test]
    fn remove_chunk_natural_light_neighborhood() {
        /*
                           Chunk               Neighbor                    Chunk               Neighbor
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     11 | -- | 15 |        | -- | -- | 15 |          11 | -- | 15 |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     10 | -- | 15 |        | 14 | -- | 15 |          10 | -- | 15 |        | 14 | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     9  | -- | -- |        | 13 | -- | 15 |          9  | -- | -- |        | --*| -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     8  | -- | 11 |        | 12 | -- | 15 |          8  | -- | 0  |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     7  | -- | 10 |        | -- | -- | 15 |          7  | -- | 0  |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     6  | -- | 9  |        | 8  | -- | 15 |    ->    6  | -- | 0  |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     5  | -- | -- |        | 7  | -- | 15 |          5  | -- | -- |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     4  | -- | 8  |        | 7  | -- | 15 |          4  | -- | 0  |        | 0  | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     3  | -- | 9  |        | -- | -- | 15 |          3  | -- | 0  |        | -- | -- | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+
        Y            2  | -- | 10 |        | 11 | -- | 15 |          2  | -- | 0  |        | --*| -- | 15 |
        |               +----+----+        +----+----+----+             +----+----+        +----+----+----+
        |            1  | -- | -- |        | 12 | -- | 15 |          1  | -- | -- |        | 12 | -- | 15 |
        + ---- X        +----+----+        +----+----+----+             +----+----+        +----+----+----+
                     0  | -- | 12 |        | 13 | 14 | 15 |          0  | -- | 12 |        | 13 | 14 | 15 |
                        +----+----+        +----+----+----+             +----+----+        +----+----+----+

                     +    14   15            0    1    2             +    14   15            0    1    2
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
        chunk.kinds.set((15, 10, 0).into(), 0.into());
        chunk.kinds.set((15, 8, 0).into(), 0.into());
        chunk.kinds.set((15, 7, 0).into(), 0.into());
        chunk.kinds.set((15, 6, 0).into(), 0.into());
        chunk.kinds.set((15, 4, 0).into(), 0.into());
        chunk.kinds.set((15, 3, 0).into(), 0.into());
        chunk.kinds.set((15, 2, 0).into(), 0.into());
        chunk.kinds.set((15, 0, 0).into(), 0.into());

        neighbor.kinds.set((0, 10, 0).into(), 0.into());
        neighbor.kinds.set((0, 9, 0).into(), 0.into());
        neighbor.kinds.set((0, 8, 0).into(), 0.into());
        neighbor.kinds.set((0, 6, 0).into(), 0.into());
        neighbor.kinds.set((0, 5, 0).into(), 0.into());
        neighbor.kinds.set((0, 4, 0).into(), 0.into());
        neighbor.kinds.set((0, 2, 0).into(), 0.into());
        neighbor.kinds.set((0, 1, 0).into(), 0.into());
        neighbor.kinds.set((0, 0, 0).into(), 0.into());
        neighbor.kinds.set((1, 0, 0).into(), 0.into());

        set_natural_light_on_top_voxels(&mut neighbor);
        set_natural_light_on_top_voxels(&mut chunk);

        world.add((0, 0, 0).into(), chunk);
        world.add((1, 0, 0).into(), neighbor);

        super::super::update_kind_neighborhoods(
            &mut world,
            vec![(0, 0, 0).into(), (1, 0, 0).into()].iter(),
        );
        super::propagate_natural_light_on_new_chunks(
            &mut world,
            &vec![(0, 0, 0).into(), (1, 0, 0).into()],
        );

        assert_eq!(
            world
                .get((1, 0, 0).into())
                .unwrap()
                .lights
                .get_natural((0, 0, 0).into()),
            13,
            "Light propagation failed. This is handled in another test"
        );

        assert_eq!(
            world
                .get((0, 0, 0).into())
                .unwrap()
                .lights
                .get_natural((15, 0, 0).into()),
            12,
            "Light propagation failed. This is handled in another test"
        );

        let neighbor = world.get_mut((1, 0, 0).into()).unwrap();

        neighbor.kinds.set((0, 9, 0).into(), 1.into());
        neighbor.kinds.set((0, 2, 0).into(), 1.into());

        let mut voxels = HashMap::new();
        voxels.insert((1, 0, 0).into(), vec![(0, 9, 0).into(), (0, 2, 0).into()]);

        drop(neighbor);

        //Act
        super::remove_natural_light(&mut world, voxels);

        // Check neighbor
        let neighbor = world.get((1, 0, 0).into()).unwrap();
        let expected_neighbor = [
            ((0, 0, 0).into(), 13),
            ((0, 1, 0).into(), 12),
            ((0, 4, 0).into(), 0),
            ((0, 5, 0).into(), 0),
            ((0, 6, 0).into(), 0),
            ((0, 8, 0).into(), 0),
        ];

        for (voxel, intensity) in expected_neighbor {
            assert_eq!(
                neighbor.lights.get_natural(voxel),
                intensity,
                "Failed at {voxel}"
            );
        }

        // Check chunk
        let chunk = world.get((0, 0, 0).into()).unwrap();
        let expected_chunk = [
            ((15, 0, 0).into(), 12),
            ((15, 2, 0).into(), 0),
            ((15, 3, 0).into(), 0),
            ((15, 4, 0).into(), 0),
            ((15, 6, 0).into(), 0),
            ((15, 7, 0).into(), 0),
            ((15, 8, 0).into(), 0),
        ];

        for (voxel, intensity) in expected_chunk {
            assert_eq!(
                chunk.lights.get_natural(voxel),
                intensity,
                "Failed at {voxel}"
            );
        }
    }

    #[test]
    fn remove_chunk_natural_light_simple() {
        /*
                        +------------------------+      +-----------------------------+
                     4  | 15 | 15 | 15 | 15 | 15 |      | 15 | 15 | 15 | 15 | 15 | 15 |
                        +------------------------+      +-----------------------------+
                     3  | 15 | 15 | 15 | 15 | 15 |      | -- | -- | -- | -- | 15 | 15 |
                        +------------------------+      +-----------------------------+
        Y            2  | 15 | 15 | 15 | 15 | 15 |  ->  | -- | 12 | 13 | 14 | 15 | 15 |
        |               +------------------------+      +-----------------------------+
        |            1  | 15 | 15 | 15 | 15 | 15 |      | -- | 11 | -- | -- | -- | 15 |
        + ---- X        +------------------------+      +-----------------------------+
                     0  | 15 | 15 | 15 | 15 | 15 |      | -- | 10 | 9  | 8  | 7  | -- |
                        +------------------------+      +-----------------------------+

                     +    0    1    2    3    4      +    0    1    2    3    4    5
        */

        let mut chunk = Chunk::default();

        // Fill all blocks on Z = 1 so we can ignore the third dimension when propagating the light
        fill_z_axis(1, &mut chunk);
        set_natural_light_on_top_voxels(&mut chunk);
        super::propagate_chunk_natural_light(&mut chunk, &top_voxels().collect::<Vec<_>>(), false);

        for x in 0..=chunk::X_END {
            for y in 0..=chunk::Y_END {
                assert_eq!(
                    chunk.lights.get_natural((x, y, 0).into()),
                    voxel::Light::MAX_NATURAL_INTENSITY
                );
            }
        }

        chunk.kinds.set((0, 0, 0).into(), 1.into());
        chunk.kinds.set((0, 1, 0).into(), 1.into());
        chunk.kinds.set((0, 2, 0).into(), 1.into());
        chunk.kinds.set((0, 3, 0).into(), 1.into());
        chunk.kinds.set((1, 3, 0).into(), 1.into());
        chunk.kinds.set((2, 1, 0).into(), 1.into());
        chunk.kinds.set((2, 3, 0).into(), 1.into());
        chunk.kinds.set((3, 1, 0).into(), 1.into());
        chunk.kinds.set((3, 3, 0).into(), 1.into());
        chunk.kinds.set((4, 1, 0).into(), 1.into());
        chunk.kinds.set((5, 0, 0).into(), 1.into());

        chunk.lights.set_natural((0, 0, 0).into(), 0);
        chunk.lights.set_natural((0, 1, 0).into(), 0);
        chunk.lights.set_natural((0, 2, 0).into(), 0);
        chunk.lights.set_natural((0, 3, 0).into(), 0);
        chunk.lights.set_natural((1, 3, 0).into(), 0);
        chunk.lights.set_natural((2, 1, 0).into(), 0);
        chunk.lights.set_natural((2, 3, 0).into(), 0);
        chunk.lights.set_natural((3, 1, 0).into(), 0);
        chunk.lights.set_natural((3, 3, 0).into(), 0);
        chunk.lights.set_natural((4, 1, 0).into(), 0);

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);

        let mut chunk_map = ChunkVoxelMap::new();
        chunk_map.insert(
            (0, 0, 0).into(),
            vec![
                (0, 0, 0).into(),
                (0, 1, 0).into(),
                (0, 2, 0).into(),
                (0, 3, 0).into(),
                (1, 3, 0).into(),
                (2, 1, 0).into(),
                (2, 3, 0).into(),
                (3, 1, 0).into(),
                (3, 3, 0).into(),
                (4, 1, 0).into(),
            ],
        );

        super::remove_natural_light(&mut world, chunk_map);

        let expected = vec![
            ((0, 0, 0).into(), 0),
            ((0, 1, 0).into(), 0),
            ((0, 2, 0).into(), 0),
            ((0, 3, 0).into(), 0),
            ((0, 4, 0).into(), 15),
            ((1, 0, 0).into(), 10),
            ((1, 1, 0).into(), 11),
            ((1, 2, 0).into(), 12),
            ((1, 3, 0).into(), 0),
            ((1, 4, 0).into(), 15),
            ((2, 0, 0).into(), 9),
            ((2, 1, 0).into(), 0),
            ((2, 2, 0).into(), 13),
            ((2, 3, 0).into(), 0),
            ((2, 4, 0).into(), 15),
            ((3, 0, 0).into(), 8),
            ((3, 1, 0).into(), 0),
            ((3, 2, 0).into(), 14),
            ((3, 3, 0).into(), 0),
            ((3, 4, 0).into(), 15),
            ((4, 0, 0).into(), 7),
            ((4, 1, 0).into(), 0),
            ((4, 2, 0).into(), 15),
            ((4, 3, 0).into(), 15),
            ((4, 4, 0).into(), 15),
        ];

        let chunk = world.get((0, 0, 0).into()).unwrap();
        for (voxel, intensity) in expected {
            assert_eq!(
                chunk.lights.get_natural(voxel),
                intensity,
                "Failed at {voxel}"
            );
        }
    }

    #[test]
    fn propagate_chunk_natural_light_simple_blocked() {
        /*
                        +------------------------+
                     4  | 15 | 15 | 15 | 15 | 15 |
                        +------------------------+
                     3  | 15 | 15 | -- | 15 | 15 |
                        +------------------------+
        Y            2  | 15 | 15 | 14 | 15 | 15 |
        |               +------------------------+
        |            1  | 15 | 15 | 14 | 15 | 15 |
        + ---- X        +------------------------+
                     0  | 15 | 15 | 14 | 15 | 15 |
                        +------------------------+

                     +    0    1    2    3    4
        */

        let mut chunk = Chunk::default();

        set_natural_light_on_top_voxels(&mut chunk);

        chunk.kinds.set((2, 3, 0).into(), 1.into());

        super::propagate_chunk_natural_light(&mut chunk, &top_voxels().collect::<Vec<_>>(), false);

        assert_eq!(
            chunk.lights.get((2, 3, 0).into()).get_greater_intensity(),
            0,
            "There should be no light on solid blocks"
        );

        assert_eq!(
            chunk.lights.get((2, 2, 0).into()).get_greater_intensity(),
            Light::MAX_NATURAL_INTENSITY - 1,
        );

        assert_eq!(
            chunk.lights.get((2, 1, 0).into()).get_greater_intensity(),
            Light::MAX_NATURAL_INTENSITY - 1,
        );

        assert_eq!(
            chunk.lights.get((2, 0, 0).into()).get_greater_intensity(),
            Light::MAX_NATURAL_INTENSITY - 1,
        );
    }

    #[test]
    fn propagate_chunk_natural_light_complex_blocked() {
        /*
                        +-----------------------------+----+----+
                     7  | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 15 |
                        +-----------------------------+----+----+
                     6  | 15 | 15 | -- | -- | -- | -- | -- | 15 |
                        +-----------------------------+----+----+
                     5  | 15 | -- | 10 | 9  | 8  | 7  | 6  | -- |
                        +-----------------------------+----+----+
                     4  | 15 | -- | 11 | -- | 7  | -- | 5  | -- |
                        +-----------------------------+----+----+
                     3  | 15 | -- | 12 | -- | 6  | -- | 4  | -- |
                        +-----------------------------+----+----+
        Y            2  | 15 | 14 | 13 | -- | 5  | -- | 3  | -- |
        |               +-----------------------------+----+----+
        |            1  | -- | -- | -- | -- | 4  | 3  | 2  | -- |
        + ---- X        +-----------------------------+----+----+
                     0  | -- | 0  | 1  | 2  | 3  | 2  | 1  | -- |
                        +-----------------------------+----+----+

                     +    0    1    2    3    4    5    6    7
        */

        let mut chunk = Chunk::default();

        // Fill all blocks on Z = 1 so we can ignore the third dimension when propagating the light
        fill_z_axis(1, &mut chunk);

        set_natural_light_on_top_voxels(&mut chunk);

        chunk.kinds.set((0, 0, 0).into(), 1.into());
        chunk.kinds.set((0, 1, 0).into(), 1.into());
        chunk.kinds.set((1, 1, 0).into(), 1.into());
        chunk.kinds.set((1, 3, 0).into(), 1.into());
        chunk.kinds.set((1, 4, 0).into(), 1.into());
        chunk.kinds.set((1, 5, 0).into(), 1.into());
        chunk.kinds.set((2, 1, 0).into(), 1.into());
        chunk.kinds.set((2, 6, 0).into(), 1.into());
        chunk.kinds.set((3, 1, 0).into(), 1.into());
        chunk.kinds.set((3, 2, 0).into(), 1.into());
        chunk.kinds.set((3, 3, 0).into(), 1.into());
        chunk.kinds.set((3, 4, 0).into(), 1.into());
        chunk.kinds.set((3, 6, 0).into(), 1.into());
        chunk.kinds.set((4, 6, 0).into(), 1.into());
        chunk.kinds.set((5, 2, 0).into(), 1.into());
        chunk.kinds.set((5, 3, 0).into(), 1.into());
        chunk.kinds.set((5, 4, 0).into(), 1.into());
        chunk.kinds.set((5, 6, 0).into(), 1.into());
        chunk.kinds.set((6, 6, 0).into(), 1.into());
        chunk.kinds.set((7, 0, 0).into(), 1.into());
        chunk.kinds.set((7, 1, 0).into(), 1.into());
        chunk.kinds.set((7, 2, 0).into(), 1.into());
        chunk.kinds.set((7, 3, 0).into(), 1.into());
        chunk.kinds.set((7, 4, 0).into(), 1.into());
        chunk.kinds.set((7, 5, 0).into(), 1.into());

        super::propagate_chunk_natural_light(&mut chunk, &top_voxels().collect::<Vec<_>>(), false);

        let expected = [
            ((0, 0, 0).into(), 0),
            ((0, 1, 0).into(), 0),
            ((0, 2, 0).into(), 15),
            ((0, 3, 0).into(), 15),
            ((0, 4, 0).into(), 15),
            ((0, 5, 0).into(), 15),
            ((0, 6, 0).into(), 15),
            ((0, 7, 0).into(), 15),
            ((1, 0, 0).into(), 0),
            ((1, 1, 0).into(), 0),
            ((1, 2, 0).into(), 14),
            ((1, 3, 0).into(), 0),
            ((1, 4, 0).into(), 0),
            ((1, 5, 0).into(), 0),
            ((1, 6, 0).into(), 15),
            ((1, 7, 0).into(), 15),
            ((2, 0, 0).into(), 1),
            ((2, 1, 0).into(), 0),
            ((2, 2, 0).into(), 13),
            ((2, 3, 0).into(), 12),
            ((2, 4, 0).into(), 11),
            ((2, 5, 0).into(), 10),
            ((2, 6, 0).into(), 0),
            ((2, 7, 0).into(), 15),
            ((3, 0, 0).into(), 2),
            ((3, 1, 0).into(), 0),
            ((3, 2, 0).into(), 0),
            ((3, 3, 0).into(), 0),
            ((3, 4, 0).into(), 0),
            ((3, 5, 0).into(), 9),
            ((3, 6, 0).into(), 0),
            ((3, 7, 0).into(), 15),
            ((4, 0, 0).into(), 3),
            ((4, 1, 0).into(), 4),
            ((4, 2, 0).into(), 5),
            ((4, 3, 0).into(), 6),
            ((4, 4, 0).into(), 7),
            ((4, 5, 0).into(), 8),
            ((4, 6, 0).into(), 0),
            ((4, 7, 0).into(), 15),
            ((5, 0, 0).into(), 2),
            ((5, 1, 0).into(), 3),
            ((5, 2, 0).into(), 0),
            ((5, 3, 0).into(), 0),
            ((5, 4, 0).into(), 0),
            ((5, 5, 0).into(), 7),
            ((5, 6, 0).into(), 0),
            ((5, 7, 0).into(), 15),
            ((6, 0, 0).into(), 1),
            ((6, 1, 0).into(), 2),
            ((6, 2, 0).into(), 3),
            ((6, 3, 0).into(), 4),
            ((6, 4, 0).into(), 5),
            ((6, 5, 0).into(), 6),
            ((6, 6, 0).into(), 0),
            ((6, 7, 0).into(), 15),
            ((7, 0, 0).into(), 0),
            ((7, 1, 0).into(), 0),
            ((7, 2, 0).into(), 0),
            ((7, 3, 0).into(), 0),
            ((7, 4, 0).into(), 0),
            ((7, 5, 0).into(), 0),
            ((7, 6, 0).into(), 15),
            ((7, 7, 0).into(), 15),
        ];

        for (local, intensity) in expected {
            assert_eq!(
                chunk.lights.get(local).get_greater_intensity(),
                intensity,
                "Failed at local {:?}",
                local
            );
        }

        /*
                        +-----------------------------+----+----+
                     7  | 15 | 15 | 15 | 15 | 15 | 15 | 15 | 15 |
                        +-----------------------------+----+----+
                     6  | 15 | 15 | -- | -- | -- | -- | -- | 15 |
                        +-----------------------------+----+----+
                     5  | 15 | -- | 10 | 11 | 12 | 13 | 14 | 15 |
                        +-----------------------------+----+----+
                     4  | 15 | -- | 11 | -- | 11 | -- | 13 | -- |
                        +-----------------------------+----+----+
                     3  | 15 | -- | 12 | -- | 10 | -- | 12 | -- |
                        +-----------------------------+----+----+
        Y            2  | 15 | 14 | 13 | -- | 9  | -- | 11 | -- |
        |               +-----------------------------+----+----+
        |            1  | -- | -- | -- | -- | 8  | 9  | 10 | -- |
        + ---- X        +-----------------------------+----+----+
                     0  | -- | 4  | 5  | 6  | 7  | 8  | 9  | -- |
                        +-----------------------------+----+----+

                     +    0    1    2    3    4    5    6    7
        */

        // Allow light to enter on (7, 5)
        chunk.kinds.set((7, 5, 0).into(), 0.into());
        chunk.lights.set(
            (7, 5, 0).into(),
            Light::natural(Light::MAX_NATURAL_INTENSITY),
        );

        super::propagate_chunk_natural_light(&mut chunk, &[(7, 5, 0).into()], false);

        let expected = [
            ((0, 0, 0).into(), 0),
            ((0, 1, 0).into(), 0),
            ((0, 2, 0).into(), 15),
            ((0, 3, 0).into(), 15),
            ((0, 4, 0).into(), 15),
            ((0, 5, 0).into(), 15),
            ((0, 6, 0).into(), 15),
            ((0, 7, 0).into(), 15),
            ((1, 0, 0).into(), 4),
            ((1, 1, 0).into(), 0),
            ((1, 2, 0).into(), 14),
            ((1, 3, 0).into(), 0),
            ((1, 4, 0).into(), 0),
            ((1, 5, 0).into(), 0),
            ((1, 6, 0).into(), 15),
            ((1, 7, 0).into(), 15),
            ((2, 0, 0).into(), 5),
            ((2, 1, 0).into(), 0),
            ((2, 2, 0).into(), 13),
            ((2, 3, 0).into(), 12),
            ((2, 4, 0).into(), 11),
            ((2, 5, 0).into(), 10),
            ((2, 6, 0).into(), 0),
            ((2, 7, 0).into(), 15),
            ((3, 0, 0).into(), 6),
            ((3, 1, 0).into(), 0),
            ((3, 2, 0).into(), 0),
            ((3, 3, 0).into(), 0),
            ((3, 4, 0).into(), 0),
            ((3, 5, 0).into(), 11),
            ((3, 6, 0).into(), 0),
            ((3, 7, 0).into(), 15),
            ((4, 0, 0).into(), 7),
            ((4, 1, 0).into(), 8),
            ((4, 2, 0).into(), 9),
            ((4, 3, 0).into(), 10),
            ((4, 4, 0).into(), 11),
            ((4, 5, 0).into(), 12),
            ((4, 6, 0).into(), 0),
            ((4, 7, 0).into(), 15),
            ((5, 0, 0).into(), 8),
            ((5, 1, 0).into(), 9),
            ((5, 2, 0).into(), 0),
            ((5, 3, 0).into(), 0),
            ((5, 4, 0).into(), 0),
            ((5, 5, 0).into(), 13),
            ((5, 6, 0).into(), 0),
            ((5, 7, 0).into(), 15),
            ((6, 0, 0).into(), 9),
            ((6, 1, 0).into(), 10),
            ((6, 2, 0).into(), 11),
            ((6, 3, 0).into(), 12),
            ((6, 4, 0).into(), 13),
            ((6, 5, 0).into(), 14),
            ((6, 6, 0).into(), 0),
            ((6, 7, 0).into(), 15),
            ((7, 0, 0).into(), 0),
            ((7, 1, 0).into(), 0),
            ((7, 2, 0).into(), 0),
            ((7, 3, 0).into(), 0),
            ((7, 4, 0).into(), 0),
            ((7, 5, 0).into(), 15),
            ((7, 6, 0).into(), 15),
            ((7, 7, 0).into(), 15),
        ];

        for (local, intensity) in expected {
            assert_eq!(
                chunk.lights.get(local).get_greater_intensity(),
                intensity,
                "Failed at local {:?}",
                local
            );
        }
    }

    #[test]
    fn propagate_natural_light_new_chunk_neighborhood() {
        /*
                           Chunk             Neighbor
                        +----+----+        +----+----+
                     11 | -- | 15 |        | -- | -- |
                        +----+----+        +----+----+
                     10 | -- | 15 |        | 14 | -- |
                        +----+----+        +----+----+
                     9  | -- | -- |        | 13 | -- |
                        +----+----+        +----+----+
                     8  | -- | 11 |        | 12 | -- |
                        +----+----+        +----+----+
                     7  | -- | 10 |        | -- | -- |
                        +----+----+        +----+----+
                     6  | -- | 9  |        | 8  | -- |
                        +----+----+        +----+----+
                     5  | -- | -- |        | 7  | -- |
                        +----+----+        +----+----+
                     4  | -- | 5  |        | 6  | -- |
                        +----+----+        +----+----+
                     3  | -- | 4  |        | -- | -- |
                        +----+----+        +----+----+
        Y            2  | -- | 3  |        | 2  | -- |
        |               +----+----+        +----+----+
        |            1  | -- | -- |        | 1  | -- |
        + ---- X        +----+----+        +----+----+
                     0  | -- | 0  |        | 0  | -- |
                        +----+----+        +----+----+

                     +    14   15            0    1
        */

        let mut world = VoxWorld::default();

        let mut chunk = Chunk::default();
        chunk.kinds.set_all(1.into()); // Make solid

        let mut neighbor = Chunk::default();
        neighbor.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (11..=chunk::Y_END).rev() {
            chunk.kinds.set((15, y, 0).into(), 0.into());
        }

        chunk.kinds.set((15, 11, 0).into(), 0.into());
        chunk.kinds.set((15, 10, 0).into(), 0.into());
        chunk.kinds.set((15, 8, 0).into(), 0.into());
        chunk.kinds.set((15, 7, 0).into(), 0.into());
        chunk.kinds.set((15, 6, 0).into(), 0.into());
        chunk.kinds.set((15, 4, 0).into(), 0.into());
        chunk.kinds.set((15, 3, 0).into(), 0.into());
        chunk.kinds.set((15, 2, 0).into(), 0.into());
        chunk.kinds.set((15, 0, 0).into(), 0.into());

        neighbor.kinds.set((0, 10, 0).into(), 0.into());
        neighbor.kinds.set((0, 9, 0).into(), 0.into());
        neighbor.kinds.set((0, 8, 0).into(), 0.into());
        neighbor.kinds.set((0, 6, 0).into(), 0.into());
        neighbor.kinds.set((0, 5, 0).into(), 0.into());
        neighbor.kinds.set((0, 4, 0).into(), 0.into());
        neighbor.kinds.set((0, 2, 0).into(), 0.into());
        neighbor.kinds.set((0, 1, 0).into(), 0.into());
        neighbor.kinds.set((0, 0, 0).into(), 0.into());

        // Set light only on chunk, so it can propagate all the way down.
        chunk.lights.set(
            (15, chunk::Y_END, 0).into(),
            Light::natural(Light::MAX_NATURAL_INTENSITY),
        );

        world.add((0, 0, 0).into(), chunk);
        world.add((1, 0, 0).into(), neighbor);

        super::super::update_kind_neighborhoods(
            &mut world,
            vec![(0, 0, 0).into(), (1, 0, 0).into()].iter(),
        );
        super::propagate_natural_light_on_new_chunks(
            &mut world,
            &vec![(0, 0, 0).into(), (1, 0, 0).into()],
        );

        let chunk_expected = [
            ((15, 11, 0).into(), 15),
            ((15, 10, 0).into(), 15),
            ((15, 8, 0).into(), 11),
            ((15, 7, 0).into(), 10),
            ((15, 6, 0).into(), 9),
            ((15, 4, 0).into(), 5),
            ((15, 3, 0).into(), 4),
            ((15, 2, 0).into(), 3),
            ((15, 0, 0).into(), 0),
        ];

        let chunk = world.get((0, 0, 0).into()).unwrap();

        for (local, intensity) in chunk_expected {
            assert_eq!(
                chunk.lights.get(local).get(voxel::LightTy::Natural),
                intensity,
                "Failed at {:?}",
                local
            );
        }

        let neighbor_expected = [
            ((0, 10, 0).into(), 14),
            ((0, 9, 0).into(), 13),
            ((0, 8, 0).into(), 12),
            ((0, 6, 0).into(), 8),
            ((0, 5, 0).into(), 7),
            ((0, 4, 0).into(), 6),
            ((0, 2, 0).into(), 2),
            ((0, 1, 0).into(), 1),
            ((0, 0, 0).into(), 0),
        ];

        let neighbor = world.get((1, 0, 0).into()).unwrap();

        for (local, intensity) in neighbor_expected {
            assert_eq!(
                neighbor.lights.get(local).get(voxel::LightTy::Natural),
                intensity,
                "Failed at {:?}",
                local
            );
        }
    }
}
