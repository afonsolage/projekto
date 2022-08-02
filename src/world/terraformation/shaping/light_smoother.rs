use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::world::{
    storage::{
        chunk::{self, ChunkStorage, ChunkStorageType},
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

    pub fn get(&self, side: voxel::Side) -> [f32; 4] {
        self.0[side as usize]
    }
}

impl ChunkStorageType for SmoothLight {}

pub type ChunkSmoothLight = ChunkStorage<SmoothLight>;

#[derive(Debug, Copy, Clone, Default)]
enum NeighborLight {
    #[default]
    Opaque,
    Transparent(u8),
}

impl NeighborLight {
    fn is_opaque(&self) -> bool {
        match self {
            NeighborLight::Opaque => true,
            NeighborLight::Transparent(_) => false,
        }
    }

    fn intensity(self) -> u8 {
        match self {
            NeighborLight::Opaque => 0,
            NeighborLight::Transparent(i) => i,
        }
    }
}

fn gather_neighborhood_light(
    world: &VoxWorld,
    local: IVec3,
    voxel: IVec3,
) -> [NeighborLight; NEIGHBOR_COUNT] {
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

    let mut neighbors = [default(); NEIGHBOR_COUNT];

    let chunk = world
        .get(local)
        .expect("Light smoothing should be done only on existing chunks");

    let mut i = 0;
    for y in -1..=1 {
        for z in -1..=1 {
            for x in -1..=1 {
                let dir = IVec3::new(x, y, z);

                if dir == IVec3::ZERO {
                    continue;
                }

                let side_voxel = voxel + dir;

                let intensity = if chunk::is_within_bounds(side_voxel) {
                    let intensity = chunk.lights.get(side_voxel).get_greater_intensity();

                    // Check if returned block is opaque
                    if intensity == 0 && chunk.kinds.get(side_voxel).is_opaque() {
                        NeighborLight::Opaque
                    } else {
                        NeighborLight::Transparent(intensity)
                    }
                } else {
                    let (dir, neighbor_voxel) = chunk::overlap_voxel(side_voxel);
                    let neighbor_local = local + dir;

                    if let Some(neighbor_chunk) = world.get(neighbor_local) {
                        let intensity = neighbor_chunk
                            .lights
                            .get(neighbor_voxel)
                            .get_greater_intensity();

                        // Check if returned block is opaque
                        if intensity == 0 && neighbor_chunk.kinds.get(neighbor_voxel).is_opaque() {
                            NeighborLight::Opaque
                        } else {
                            NeighborLight::Transparent(intensity)
                        }
                    } else {
                        // TODO: When a neighbor chunk isn't loaded we should make it lighter or darker?
                        NeighborLight::Transparent(0)
                    }
                };

                neighbors[i] = intensity;

                i += 1;
            }
        }
    }

    neighbors
}

fn smooth_ambient_occlusion(
    neighbors: &[NeighborLight; NEIGHBOR_COUNT],
    side: voxel::Side,
    vertex: usize,
    emitter: bool,
) -> f32 {
    let idx = side as usize;
    let side = neighbors[NEIGHBOR_VERTEX_LOOKUP[idx][vertex][0]];

    // Light emitter doesn't have ambient occlusion nor light smoothing.
    if emitter {
        return side.intensity() as f32;
    }

    let side1 = neighbors[NEIGHBOR_VERTEX_LOOKUP[idx][vertex][1]];
    let side2 = neighbors[NEIGHBOR_VERTEX_LOOKUP[idx][vertex][2]];
    let corner = neighbors[NEIGHBOR_VERTEX_LOOKUP[idx][vertex][3]];

    let corner = if side1.is_opaque() && side2.is_opaque() {
        NeighborLight::Opaque
    } else {
        corner
    };

    // Convert from i32, which has the info if the voxel is opaque, to pure light intensity
    (side.intensity() + side1.intensity() + side2.intensity() + corner.intensity()) as f32 / 4.0
}

pub fn smooth_lighting(
    world: &VoxWorld,
    local: IVec3,
    occlusion: &ChunkFacesOcclusion,
) -> ChunkSmoothLight {
    let mut chunk_smooth_light = ChunkSmoothLight::default();

    if let Some(chunk) = world.get(local) {
        for voxel in chunk::voxels() {
            let occlusion = occlusion.get(voxel);

            if occlusion.is_fully_occluded() {
                continue;
            }

            let emitter = chunk.kinds.get(voxel).is_light_emitter();

            let neighbors = gather_neighborhood_light(world, local, voxel);
            let mut smooth_light = SmoothLight::default();

            for side in voxel::SIDES {
                if occlusion.is_occluded(side) {
                    continue;
                }

                smooth_light.set(
                    side,
                    [
                        smooth_ambient_occlusion(&neighbors, side, 0, emitter),
                        smooth_ambient_occlusion(&neighbors, side, 1, emitter),
                        smooth_ambient_occlusion(&neighbors, side, 2, emitter),
                        smooth_ambient_occlusion(&neighbors, side, 3, emitter),
                    ],
                );
            }

            chunk_smooth_light.set(voxel, smooth_light);
        }
    }

    chunk_smooth_light
}

#[cfg(test)]
mod tests {
    use crate::world::storage::{chunk::Chunk, voxel::Light};

    use super::*;

    #[test]
    fn gather_neighborhood_light() {
        let mut chunk = Chunk::default();

        let voxel = IVec3::new(10, 10, 10);

        let mut i = 0;
        for y in -1..=1 {
            for z in -1..=1 {
                for x in -1..=1 {
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
        for y in -1..=1 {
            for z in -1..=1 {
                for x in -1..=1 {
                    let neighbor = voxel + IVec3::new(x, y, z);

                    if neighbor == voxel {
                        continue;
                    }

                    assert_eq!(
                        chunk.lights.get(neighbor).get_greater_intensity(),
                        neighbors[i].intensity(),
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
