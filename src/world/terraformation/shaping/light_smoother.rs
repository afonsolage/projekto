use bevy::prelude::*;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::world::{
    storage::{
        chunk::{self, Chunk, ChunkStorage, ChunkStorageType},
        voxel, VoxWorld,
    },
    terraformation::ChunkFacesOcclusion,
};

const NEIGHBOR_COUNT: usize = 26;
const VERTEX_COUNT: usize = 4;
const VERTEX_NEIGHBOR_COUNT: usize = 4;

/// Lookup table used to gather neighbor information in order to smooth lighting
/// This table is built using the following order: side, side1, side2, corner
/// side is the direct side in which the face normal is pointing to
/// side1 and side2 are adjacent to side
/// corner is relative to side also.
const NEIGHBOR_VERTEX_LOOKUP: [[[usize; VERTEX_COUNT]; VERTEX_NEIGHBOR_COUNT]; voxel::SIDE_COUNT] = [
    /*
         v3               v2
            +-----------+
      v7  / |      v6 / |
        +-----------+   |
        |   |       |   |
        |   +-------|---+
        | /  v0     | /  v1
        +-----------+
       v4           v5

       Y
       |
       +---X
      /
    Z
    */
    // RIGHT
    [
        [13, 5, 16, 8],   // v5
        [13, 5, 11, 2],   // v1
        [13, 22, 11, 19], // v2
        [13, 22, 16, 25], // v6
    ],
    // LEFT
    [
        [12, 3, 9, 0],    // v0
        [12, 3, 14, 6],   // v4
        [12, 20, 14, 23], // v7
        [12, 20, 9, 17],  // v3
    ],
    // UP
    [
        [21, 24, 20, 23], // v7
        [21, 24, 22, 25], // v6
        [21, 18, 22, 19], // v2
        [21, 18, 20, 17], // v3
    ],
    // DOWN
    [
        [4, 1, 3, 0], // v0
        [4, 1, 5, 2], // v1
        [4, 7, 5, 8], // v5
        [4, 7, 3, 6], // v4
    ],
    // FRONT
    [
        [15, 7, 14, 6],   // v4
        [15, 7, 16, 8],   // v5
        [15, 24, 16, 25], // v6
        [15, 24, 14, 23], // v7
    ],
    // BACK
    [
        [10, 1, 11, 2],   // v1
        [10, 1, 9, 0],    // v0
        [10, 18, 9, 17],  // v3
        [10, 18, 11, 19], // v2
    ],
];

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct SmoothLight([[f32; 4]; voxel::SIDE_COUNT]);
impl SmoothLight {
    fn set(&mut self, side: voxel::Side, light: [f32; 4]) {
        self.0[side as usize] = light;
    }
    fn get(&self, side: voxel::Side) -> [f32; 4] {
        self.0[side as usize]
    }
}
impl ChunkStorageType for SmoothLight {}

pub type ChunkSmoothLight = ChunkStorage<SmoothLight>;

fn absolute(world: &VoxWorld, local: IVec3, voxel: IVec3) -> u8 {
    if chunk::is_within_bounds(voxel) {
        world
            .get(local)
            .expect("Light smoothing should be done only on existing chunks")
            .lights
            .get(voxel)
            .get_greater_intensity()
    } else {
        let (dir, neighbor_voxel) = chunk::overlap_voxel(voxel);
        let neighbor_local = local + dir;

        if let Some(chunk) = world.get(neighbor_local) {
            chunk.lights.get(neighbor_voxel).get_greater_intensity()
        } else {
            // TODO: When a neighbor chunk isn't loaded we should make it lighter or darker?
            0
        }
    }
}

fn gather_neighborhood_light(world: &VoxWorld, local: IVec3, voxel: IVec3) -> [u8; NEIGHBOR_COUNT] {
    /*

                             +------+------+------+
                            /  17  /  18  /  19  /|
                           +------+------+------+ |
                          /  20  /  21  /  22  /| /
                         +------+------+------+ |/
                        /  23  /  24  /  25  /| /
                       +------+------+------+ |/
                       |      |      |      | /
                       |      |      |      |/
                       +------+------+------+
                             +------+------+------+
                            /  9   /  10  /  11  /|
                           +------+------+------+ |
                          /  12  /|     /  13  /| /
                         +------+------+------+ |/
                        /  14  /  15  /  16  /| /
                       +------+------+------+ |/
                       |      |      |      | /
                       |      |      |      |/
                       +------+------+------+
                             +------+------+------+
       Y                    /  0   /  1   /  2   /|
       |                   +------+------+------+ |
       |                  /   3  /   4  /  5   /| /
       + ---- X          +------+------+------+ |/
      /                 /   6  /   7  /  8   /| /
     Z                 +------+------+------+ |/
                       |      |      |      | /
                       |      |      |      |/
                       +------+------+------+
    */

    let mut neighbors = [0; NEIGHBOR_COUNT];

    let mut i = 0;
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                let dir = IVec3::new(x, y, z);

                if dir == IVec3::ZERO {
                    continue;
                }

                neighbors[i] = absolute(world, local, voxel + dir);
                i += 1;
            }
        }
    }

    neighbors
}

fn smooth_ambient_occlusion(side: u8, side1: u8, side2: u8, corner: u8) -> f32 {
    let corner = if side1 == 0 && side2 == 0 { 0 } else { corner };
    (side + side1 + side2 + corner) as f32 / 4.0
}

pub fn smooth_lighting(
    world: &VoxWorld,
    local: IVec3,
    occlusion: ChunkFacesOcclusion,
) -> ChunkSmoothLight {
    let mut chunk_smooth_light = ChunkSmoothLight::default();

    if world.exists(local) {
        for voxel in chunk::voxels() {
            let occlusion = occlusion.get(voxel);

            if occlusion.is_fully_occluded() {
                continue;
            }

            let neighbors = gather_neighborhood_light(world, local, voxel);
            let mut smooth_light = SmoothLight::default();

            for side in voxel::SIDES {
                if occlusion.is_occluded(side) {
                    continue;
                }

                let side_lookup = NEIGHBOR_VERTEX_LOOKUP[side as usize];

                let side_smooth_light: [f32; VERTEX_COUNT] = (0..VERTEX_COUNT)
                    .map(|vertex| {
                        let vertex_lookup = side_lookup[vertex];
                        smooth_ambient_occlusion(
                            neighbors[vertex_lookup[0]],
                            neighbors[vertex_lookup[1]],
                            neighbors[vertex_lookup[2]],
                            neighbors[vertex_lookup[3]],
                        )
                    })
                    .collect_vec()
                    .try_into()
                    .expect("There will always be 4 vertex");

                smooth_light.set(side, side_smooth_light);
            }

            chunk_smooth_light.set(voxel, smooth_light);
        }
    }

    chunk_smooth_light
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use crate::world::storage::voxel::Light;

    use super::*;

    #[test]
    fn smooth_ambient_occlusion() {
        assert_eq!(
            super::smooth_ambient_occlusion(0, 0, 0, 0),
            0.0,
            "Should return 0 when all sides are also 0"
        );
        assert_eq!(super::smooth_ambient_occlusion(0, 0, 0, 0), 0.0);
    }

    #[test]
    fn gather_neighborhood_light() {
        let mut chunk = Chunk::default();

        let voxel = IVec3::new(10, 10, 10);

        let mut i = 0;
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    chunk
                        .lights
                        .set(voxel + IVec3::new(x, y, z), Light::natural(i));
                    i += 1
                }
            }
        }

        let mut world = VoxWorld::default();
        world.add((0, 0, 0).into(), chunk);

        let neighbors = super::gather_neighborhood_light(&world, (0, 0, 0).into(), voxel);
        let chunk = world.get((0, 0, 0).into()).unwrap();

        let mut i = 0;
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let neighbor = voxel + IVec3::new(x, y, z);

                    if neighbor == voxel {
                        continue;
                    }

                    assert_eq!(
                        chunk.lights.get(neighbor).get_greater_intensity(),
                        neighbors[i],
                        "Failed at {neighbor} [{i}]"
                    );
                    i += 1
                }
            }
        }
    }

    #[test]
    fn lookup_table() {
        let mut count = vec![0; NEIGHBOR_COUNT];

        let corners = vec![0usize, 2, 19, 17, 6, 8, 25, 23];

        for s in NEIGHBOR_VERTEX_LOOKUP {
            for v in s {
                for i in v {
                    count[i] += 1;
                }
            }
        }

        for (i, cnt) in count.into_iter().enumerate() {
            let expected = if corners.contains(&i) { 3 } else { 4 };

            assert_eq!(cnt, expected, "In Lookup each neighbor should appears 4 times, except corners, which should appears 3 times.");
        }
    }
}
