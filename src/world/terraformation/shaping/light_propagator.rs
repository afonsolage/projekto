use std::collections::VecDeque;

use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};

use crate::world::storage::{
    chunk::{self, Chunk, ChunkNeighborhood},
    voxel::{self, LightTy},
    VoxWorld,
};

pub fn propagate(world: &mut VoxWorld, locals: &[IVec3]) {
    // TODO: Separate genesis light propagation from update voxel modification

    // Propagate initial top-down natural light across all given chunks.
    // This function propagate the natural light from the top-most voxels downwards.
    // This function only propagate internally, does not spread the light to neighbors.
    propagate_natural_light_top_down(world, locals);

    // Propagates natural light from/into the neighborhood
    // This function must be called after natural internal propagation, since it will check neighbors only
    propagate_natural_light_neighborhood(world, locals);
}

fn propagate_natural_light_neighborhood(world: &mut VoxWorld, locals: &[IVec3]) -> Vec<IVec3> {
    let mut queue = locals
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
        .collect::<VecDeque<_>>();

    let mut touched = HashSet::new();

    while let Some((chunk_local, voxels)) = queue.pop_front() {
        if !world.exists(chunk_local) {
            continue;
        }

        // TODO: Check if it's possible to optimize this later on
        update_chunk_light_neighborhood(world, chunk_local);

        let chunk = world.get_mut(chunk_local).unwrap();

        queue.extend(
            // This function returns which neighbor (dir) needs to be updated.
            propagate_chunk_natural_light(chunk, &voxels, true)
                .into_iter()
                .map(|(dir, voxels)| (dir + chunk_local, voxels)), // Convert the dir to local
        );

        touched.insert(chunk_local);
    }

    touched.into_iter().collect()
}

/*
fn propagate_chunk_natural_light_neighborhood(
    chunk: &mut Chunk,
    locals: &[IVec3],
) -> HashMap<IVec3, Vec<IVec3>> {
    let mut pending_propagation = HashMap::new();

    for &local in locals {
        if !chunk.kinds.get(local).is_empty() {
            // Nothing to propagate here.
            continue;
        }

        let light = chunk.lights.get(local).get(voxel::LightTy::Natural);

        for side in voxel::SIDES {
            match chunk.kinds.get_absolute(local) {
                None => continue,
                Some(kind) if !kind.is_empty() => continue,
                _ => (),
            }

            let neighbor_local = side.dir() + local;
            let neighbor_light = chunk
                .lights
                .get_absolute(neighbor_local)
                .unwrap_or_default()
                .get(voxel::LightTy::Natural);

            if light > 1 && light > neighbor_light {
                let (_, neighbor_chunk_voxel) = chunk::overlap_voxel(neighbor_local);

                pending_propagation
                    .entry(side.dir())
                    .or_insert(vec![])
                    .push(neighbor_chunk_voxel);
            } else if neighbor_light > 1 && neighbor_light > light {
                pending_propagation
                    .entry(IVec3::ZERO)
                    .or_insert(vec![])
                    .push(local);
                chunk.lights.set_natural(local, neighbor_light);
            }
        }
    }

    pending_propagation
}
*/

fn update_chunk_light_neighborhood(world: &mut VoxWorld, local: IVec3) {
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

fn propagate_natural_light_top_down(world: &mut VoxWorld, locals: &[IVec3]) {
    for &local in locals {
        let chunk = world.get_mut(local).unwrap();

        let top_voxels = (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| (x, chunk::Y_END, z).into()))
            .collect::<Vec<_>>();

        propagate_chunk_natural_light(chunk, &top_voxels, false);
    }
}

fn propagate_chunk_natural_light(
    chunk: &mut Chunk,
    locals: &[IVec3],
    propagate_to_neighbors: bool,
) -> HashMap<IVec3, Vec<IVec3>> {
    let mut queue = locals.iter().cloned().collect::<VecDeque<_>>();

    let mut touched_neighbors = HashMap::new();

    while let Some(voxel) = queue.pop_front() {
        if !chunk.kinds.get(voxel).is_empty() {
            continue;
        }

        let current_intensity = chunk.lights.get_natural(voxel);

        for side in voxel::SIDES {
            let dir = side.dir();
            let side_voxel = voxel + dir;

            // If side voxel isn't empty, there nothing to do here.
            match chunk.kinds.get_absolute(side_voxel) {
                Some(k) if !k.is_empty() => continue,
                None => continue,
                _ => (),
            }

            let current_propagated_intensity = if side == voxel::Side::Down
                && current_intensity == voxel::Light::MAX_NATURAL_INTENSITY
            {
                current_intensity
            } else if current_intensity > 0 {
                current_intensity - 1
            } else {
                0
            };

            if chunk::is_within_bounds(side_voxel) {
                // Propagate inside the chunk
                if current_propagated_intensity > chunk.lights.get_natural(side_voxel) {
                    chunk
                        .lights
                        .set_natural(side_voxel, current_propagated_intensity);

                    if current_propagated_intensity > 1 {
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

                let neighbor_propagated_intensity = if side == voxel::Side::Down
                    && neighbor_intensity == voxel::Light::MAX_NATURAL_INTENSITY
                {
                    neighbor_intensity
                } else if neighbor_intensity > 1 {
                    neighbor_intensity - 1
                } else {
                    0
                };

                if neighbor_propagated_intensity > current_intensity {
                    // Propagate neighbor light to current voxel
                    chunk
                        .lights
                        .set_natural(voxel, neighbor_propagated_intensity);

                    // Queue back for propagation
                    if neighbor_propagated_intensity > 1 {
                        queue.push_back(voxel);
                    }
                } else if current_propagated_intensity > neighbor_intensity {
                    // Flag neighbor to propagate light on verified voxel

                    touched_neighbors
                        .entry(side.dir())
                        .or_insert(vec![])
                        .push(neighbor_voxel);
                }
            }
        }
    }

    touched_neighbors
}

pub type ChunkVoxelMap = HashMap<IVec3, Vec<IVec3>>;

pub fn remove_natural_light(world: &mut VoxWorld, voxels: ChunkVoxelMap) {
    let mut remove_queue = voxels.into_iter().collect::<VecDeque<_>>();
    let mut propagate_queue = VecDeque::<(IVec3, Vec<IVec3>)>::new();

    while let Some((local, voxels)) = remove_queue.pop_front() {
        // TODO: Check if it's possible to optimize this later on
        update_chunk_light_neighborhood(world, local);

        if let Some(chunk) = world.get_mut(local) {
            let RemoveChunkNaturalLightResult { remove, propagate } =
                remove_chunk_natural_light(chunk, &voxels);

            remove_queue.extend(
                remove
                    .into_iter()
                    .map(|(dir, voxels)| (dir + local, voxels)),
            );
            propagate_queue.extend(
                propagate
                    .into_iter()
                    .map(|(dir, voxels)| (dir + local, voxels)),
            );
        }
    }

    while let Some((local, voxels)) = propagate_queue.pop_front() {
        // TODO: Check if it's possible to optimize this later on
        update_chunk_light_neighborhood(world, local);

        if let Some(chunk) = world.get_mut(local) {
            propagate_queue.extend(
                propagate_chunk_natural_light(chunk, &voxels, true)
                    .into_iter()
                    .map(|(dir, voxels)| (dir + local, voxels)),
            );
        }
    }
}

struct RemoveChunkNaturalLightResult {
    propagate: ChunkVoxelMap,
    remove: ChunkVoxelMap,
}

fn remove_chunk_natural_light(
    chunk: &mut Chunk,
    voxels: &[IVec3],
) -> RemoveChunkNaturalLightResult {
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
                        propagate
                            .entry(side.dir())
                            .or_default()
                            .push(neighbor_voxel);
                    }
                }
            }
        }
    }

    RemoveChunkNaturalLightResult { propagate, remove }
}

#[cfg(test)]
mod tests {
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
    fn propagate_natural_light_two_neighborhood() {
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
            &vec![(0, 0, 0).into(), (1, 0, 0).into()],
        );
        super::propagate(&mut world, &vec![(0, 0, 0).into(), (1, 0, 0).into()]);

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
