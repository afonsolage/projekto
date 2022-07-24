use std::collections::VecDeque;

use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};

use crate::world::storage::{
    chunk::{self, Chunk, ChunkNeighborhood},
    voxel, VoxWorld,
};

pub fn propagate(world: &mut VoxWorld, locals: &[IVec3]) {
    propagate_natural_light(world, locals);
    update_light_neighborhood(world, locals);
    propagate_neighbors_natural_light(world, locals);
}

fn propagate_neighbors_natural_light(world: &mut VoxWorld, locals: &[IVec3]) -> Vec<IVec3> {
    // Add all neighbors of the given locals list to the queue
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

    while let Some((chunk_local, locals)) = queue.pop_front() {
        if let Some(chunk) = world.get_mut(chunk_local) {
            touched.insert(chunk_local);
            queue.extend(propagate_chunk_neighbor_natural_light(chunk, &locals))
        }
    }

    touched.into_iter().collect()
}

fn propagate_chunk_neighbor_natural_light(
    chunk: &mut Chunk,
    locals: &[IVec3],
) -> HashMap<IVec3, Vec<IVec3>> {
    let mut touched_neighbors = HashMap::new();

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

            if light > neighbor_light {
                let (_, neighbor_chunk_voxel) = chunk::overlap_voxel(neighbor_local);
                let neighbor_chunk_local = local + side.dir();

                touched_neighbors
                    .entry(neighbor_chunk_local)
                    .or_insert(vec![])
                    .push(neighbor_chunk_voxel);

                chunk.lights.set_natural(neighbor_chunk_voxel, light);
            } else if neighbor_light > light {
                chunk.lights.set_natural(local, neighbor_light);
            }
        }
    }

    touched_neighbors
}

fn update_light_neighborhood(world: &mut VoxWorld, locals: &[IVec3]) {
    for &local in locals {
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
}

fn propagate_natural_light(world: &mut VoxWorld, locals: &[IVec3]) {
    for &local in locals {
        let chunk = world.get_mut(local).unwrap();

        let top_voxels = (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| (x, chunk::Y_END, z).into()))
            .collect::<Vec<_>>();

        propagate_chunk_natural_light(chunk, &top_voxels);
    }
}

fn propagate_chunk_natural_light(chunk: &mut Chunk, locals: &[IVec3]) {
    // Create a queue with top-most voxels, to start propagation from there
    let mut queue = locals.iter().cloned().collect::<VecDeque<_>>();

    while let Some(local) = queue.pop_front() {
        let light = chunk.lights.get(local);
        let light_intensity = light.get(voxel::LightTy::Natural);

        if light_intensity <= 1 {
            continue;
        }

        for side in voxel::SIDES {
            let dir = side.dir();
            let neighbor_local = local + dir;

            // Skip neighborhood for now
            if chunk::is_within_bounds(neighbor_local) {
                // TODO: Check if kind is transparent or opaque
                if !chunk.kinds.get(neighbor_local).is_empty() {
                    continue;
                }

                let propagated_intensity = if side == voxel::Side::Down
                    && light_intensity == voxel::Light::MAX_NATURAL_INTENSITY
                {
                    light_intensity
                } else {
                    light_intensity - 1
                };

                let mut neighbor_light = chunk.lights.get(neighbor_local);

                if propagated_intensity > neighbor_light.get(voxel::LightTy::Natural) {
                    neighbor_light.set(voxel::LightTy::Natural, propagated_intensity);

                    chunk.lights.set(neighbor_local, neighbor_light);

                    if propagated_intensity > 1 {
                        queue.push_back(neighbor_local);
                    }
                }
            }
        }
    }
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
        super::propagate_chunk_neighbor_natural_light(
            &mut chunk,
            &top_voxels().collect::<Vec<_>>(),
        );

        // Test the test function
        assert_eq!(
            top_voxels().count(),
            chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE
        );

        for local in top_voxels() {
            assert_eq!(
                chunk.lights.get(local).get(voxel::LightTy::Natural),
                Light::MAX_NATURAL_INTENSITY
            );
        }
    }

    #[test]
    fn propagate_chunk_natural_light_non_empty() {
        let mut chunk = Chunk::default();

        set_natural_light_on_top_voxels(&mut chunk);

        chunk.kinds.set((1, 1, 0).into(), 1.into());

        super::propagate_chunk_neighbor_natural_light(
            &mut chunk,
            &top_voxels().collect::<Vec<_>>(),
        );

        assert_eq!(
            chunk
                .lights
                .get((1, 1, 0).into())
                .get(voxel::LightTy::Natural),
            0,
            "There should be no light on solid blocks"
        );
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

        super::propagate_chunk_natural_light(
            &mut chunk,
            &top_voxels().collect::<Vec<_>>(),
        );

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

        super::propagate_chunk_natural_light(
            &mut chunk,
            &top_voxels().collect::<Vec<_>>(),
        );

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
        chunk.kinds.set((7, 5, 0).into(), 1.into());
        chunk.lights.set(
            (7, 5, 0).into(),
            Light::natural(Light::MAX_NATURAL_INTENSITY),
        );

        super::propagate_chunk_natural_light(&mut chunk, &[(7, 5, 0).into()]);

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
}
