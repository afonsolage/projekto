use std::collections::VecDeque;

use bevy::math::IVec3;
use projekto_core::{
    math,
    voxel::{self, LightTy},
};

use crate::{
    chunk::{self, ChunkSide, ChunkStorage, GetChunkStorage},
    Chunk, Voxel,
};

/// Number of neighbors per voxel
// 3 voxels (-1..=1) per axis.
// -1 to skip self (0, 0, 0)
const NEIGHBOR_COUNT: usize = (3 * 3 * 3) - 1;
/// Vertex count per face
const VERTEX_COUNT: usize = 4;
/// Direct side, side1, side2 and corner
const VERTEX_NEIGHBOR_COUNT: usize = 4;

/// Lookup table used to gather neighbor information in order to smooth lighting
/// This table is built using the following order: side, side1, side2, corner
/// side is the direct side in which the face normal is pointing to
/// side1 and side2 are adjacent to side
/// corner is relative to side also.
const NEIGHBOR_VERTEX_LOOKUP: [[[usize; VERTEX_COUNT]; VERTEX_NEIGHBOR_COUNT]; voxel::SIDE_COUNT] = [
    //      v3          v2
    //      +---------+
    // v7  / |    v6 / |
    //   +---------+   |
    //   |   |     |   |
    //   |   +-----|---+
    //   | /  v0   | /  v1
    //   +---------+
    // v4           v5
    //
    //   Y
    //   |
    //   +---X
    //  /
    // Z
    //
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

fn gather_neighborhood_light<'a>(
    chunk: Chunk,
    voxel: Voxel,
    get_kind: impl GetChunkStorage<'a, voxel::Kind>,
    get_light: impl GetChunkStorage<'a, voxel::Light>,
) -> [Option<u8>; NEIGHBOR_COUNT] {
    let mut neighborhood = [Default::default(); NEIGHBOR_COUNT];

    let light = get_light(chunk).expect("base chunk must exists");
    let kind = get_kind(chunk).expect("base chunk must exists");

    let mut i = 0;
    for y in -1..=1 {
        for z in -1..=1 {
            for x in -1..=1 {
                let dir = IVec3::new(x, y, z);

                if dir == IVec3::ZERO {
                    continue;
                }

                let side_voxel = voxel + dir;

                let intensity = if chunk::is_inside(side_voxel) {
                    let intensity = light.get(side_voxel).get_greater_intensity();

                    // Check if returned block is opaque
                    if intensity == 0 && kind.get(side_voxel).is_opaque() {
                        None
                    } else {
                        Some(intensity)
                    }
                } else if y != 0 {
                    // There is no chunk above or below
                    Some(voxel::Light::MAX_NATURAL_INTENSITY)
                } else {
                    let (dir, neighbor_voxel) = chunk::overlap_voxel(side_voxel);
                    let neighbor_chunk = chunk.neighbor(dir);

                    // TODO: Change this when if-let chains stabilizes
                    if let Some(kind) = get_kind(neighbor_chunk) {
                        if let Some(light) = get_light(neighbor_chunk) {
                            let intensity = light.get(neighbor_voxel).get_greater_intensity();

                            // Check if returned block is opaque
                            if intensity == 0 && kind.get(neighbor_voxel).is_opaque() {
                                None
                            } else {
                                Some(intensity)
                            }
                        } else {
                            Some(voxel::Light::MAX_NATURAL_INTENSITY)
                        }
                    } else {
                        Some(voxel::Light::MAX_NATURAL_INTENSITY)
                    }
                };

                neighborhood[i] = intensity;
                i += 1;
            }
        }
    }

    neighborhood
}

/// Calculates the ambient occlusion and light smoothness based on [0fps article](https://0fps.net/2013/07/03/ambient-occlusion-for-minecraft-like-worlds/)
/// Skips AO and Light Smoothness if voxel is a light emitter
fn smooth_ambient_occlusion<const VERTEX: usize>(
    neighbors: &[Option<u8>; NEIGHBOR_COUNT],
    side: voxel::Side,
) -> f32 {
    let idx = side as usize;

    let side1 = neighbors[NEIGHBOR_VERTEX_LOOKUP[idx][VERTEX][1]];
    let side2 = neighbors[NEIGHBOR_VERTEX_LOOKUP[idx][VERTEX][2]];

    let corner = if side1.is_none() && side2.is_none() {
        0.0
    } else {
        neighbors[NEIGHBOR_VERTEX_LOOKUP[idx][VERTEX][3]].unwrap_or(0) as f32
    };

    let side = neighbors[NEIGHBOR_VERTEX_LOOKUP[idx][VERTEX][0]].unwrap_or(0) as f32;
    let side1 = side1.unwrap_or(0) as f32;
    let side2 = side2.unwrap_or(0) as f32;

    // Convert from i32, which has the info if the voxel is opaque, to pure light intensity
    (side + side1 + side2 + corner) / 4.0
}

fn soft_vertex_light(neighbors: &[Option<u8>; NEIGHBOR_COUNT], side: voxel::Side) -> [f32; 4] {
    [
        smooth_ambient_occlusion::<0>(neighbors, side),
        smooth_ambient_occlusion::<1>(neighbors, side),
        smooth_ambient_occlusion::<2>(neighbors, side),
        smooth_ambient_occlusion::<3>(neighbors, side),
    ]
}

pub fn smooth_lighting<'a>(
    chunk: Chunk,
    occlusion: &ChunkStorage<voxel::FacesOcclusion>,
    soft_light: &mut ChunkStorage<voxel::FacesSoftLight>,
    get_kind: impl GetChunkStorage<'a, voxel::Kind>,
    get_light: impl GetChunkStorage<'a, voxel::Light>,
) {
    let kind = get_kind(chunk).expect("Chunk must exists");
    let light = get_light(chunk).expect("Chunk must exists");

    chunk::voxels().for_each(|voxel| {
        if occlusion.get(voxel).is_fully_occluded() {
            return;
        }

        let faces_soft_light = if kind.get(voxel).is_light_emitter() {
            let intensity = light.get(voxel).get_greater_intensity();
            voxel::FacesSoftLight::with_intensity(intensity)
        } else {
            let voxel_occlusion = occlusion.get(voxel);
            let neighbors = gather_neighborhood_light(chunk, voxel, get_kind, get_light);
            let faces_soft_light = voxel::SIDES.map(|side| {
                if !voxel_occlusion.is_occluded(side) {
                    soft_vertex_light(&neighbors, side)
                } else {
                    Default::default()
                }
            });

            voxel::FacesSoftLight::new(faces_soft_light)
        };

        soft_light.set(voxel, faces_soft_light);
    });
}

fn calc_propagated_intensity(ty: LightTy, side: voxel::Side, intensity: u8) -> u8 {
    if side == voxel::Side::Down
        && ty == LightTy::Natural
        && intensity == voxel::Light::MAX_NATURAL_INTENSITY
    {
        voxel::Light::MAX_NATURAL_INTENSITY
    } else {
        debug_assert!(intensity > 0);
        intensity - 1
    }
}

pub struct NeighborLightPropagation {
    pub side: ChunkSide,
    pub voxel: Voxel,
    pub ty: LightTy,
    pub intensity: u8,
}

pub fn propagate(
    kind: &ChunkStorage<voxel::Kind>,
    light: &mut ChunkStorage<voxel::Light>,
    light_ty: LightTy,
    voxels: impl Iterator<Item = Voxel>,
) -> Vec<NeighborLightPropagation> {
    let mut queue = voxels.collect::<VecDeque<_>>();
    let mut neighbor_light_propagation = vec![];

    while let Some(voxel) = queue.pop_front() {
        if kind.get(voxel).is_opaque() {
            continue;
        }

        let current_intensity = light.get(voxel).get(light_ty);

        for side in voxel::SIDES {
            let propagated_intensity = calc_propagated_intensity(light_ty, side, current_intensity);

            if propagated_intensity == 0 {
                continue;
            }

            let side_voxel = voxel + side.dir();
            if !chunk::is_inside(side_voxel) {
                if let Some(chunk_side) = ChunkSide::from_voxel_side(side) {
                    let neighbor_voxel = math::euclid_rem(
                        side_voxel,
                        IVec3::new(
                            chunk::X_AXIS_SIZE as i32,
                            chunk::Y_AXIS_SIZE as i32,
                            chunk::Z_AXIS_SIZE as i32,
                        ),
                    );

                    neighbor_light_propagation.push(NeighborLightPropagation {
                        side: chunk_side,
                        voxel: neighbor_voxel,
                        ty: light_ty,
                        intensity: propagated_intensity,
                    });
                }

                continue;
            }

            let side_kind = kind.get(side_voxel);
            if side_kind.is_opaque() {
                continue;
            }

            let side_intensity = light.get(side_voxel).get(light_ty);
            if side_intensity >= propagated_intensity {
                continue;
            }

            light.set_type(side_voxel, light_ty, propagated_intensity);

            if propagated_intensity > 1 {
                queue.push_back(side_voxel);
            }
        }
    }

    neighbor_light_propagation
}

#[cfg(test)]
mod test {
    use bevy::utils::HashMap;

    use super::*;

    #[test]
    fn propagate_empty_chunk() {
        let kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        chunk::top_voxels().for_each(|voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        let _ = propagate(&kind, &mut light, LightTy::Natural, chunk::top_voxels());

        assert!(
            light
                .iter()
                .all(|l| l.get(LightTy::Natural) == voxel::Light::MAX_NATURAL_INTENSITY),
            "All voxels should have full natural light propagated"
        );
    }

    #[test]
    fn propagate_enclosed_light() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        chunk::top_voxels().for_each(|voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        kind.set(IVec3::new(1, 0, 0), 1.into());
        kind.set(IVec3::new(0, 1, 0), 1.into());
        kind.set(IVec3::new(0, 0, 1), 1.into());

        let _ = propagate(&kind, &mut light, LightTy::Natural, chunk::top_voxels());

        assert_eq!(
            light.get(IVec3::ZERO).get(LightTy::Natural),
            0,
            "Should not propagate to enclosed voxels"
        );

        assert_eq!(
            light.get(IVec3::new(1, 0, 0)).get(LightTy::Natural),
            0,
            "Should not propagate to opaque voxels"
        );

        assert_eq!(
            light.get(IVec3::new(2, 0, 0)).get(LightTy::Natural),
            voxel::Light::MAX_NATURAL_INTENSITY,
            "Should propagate at max intensity to empty voxels"
        );
    }

    #[test]
    fn propagate_partial_enclosed_light() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        chunk::top_voxels().for_each(|voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        // block all light except for a voxel at 0, 1, 0
        (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| IVec3::new(x, 1, z)))
            .skip(1)
            .for_each(|v| {
                kind.set(v, 1.into());
            });

        let _ = propagate(&kind, &mut light, LightTy::Natural, chunk::top_voxels());

        (0..=chunk::Z_END).enumerate().for_each(|(i, z)| {
            let voxel = IVec3::new(0, 0, z);
            assert_eq!(
                light.get(voxel).get(LightTy::Natural),
                voxel::Light::MAX_NATURAL_INTENSITY - i as u8,
                "Should propagate decreasing intensity at {voxel}"
            );
        });
    }

    #[test]
    fn propagate_to_neighborhood_empty_chunk() {
        let kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        chunk::top_voxels().for_each(|voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        let neighbor_propagation =
            propagate(&kind, &mut light, LightTy::Natural, chunk::top_voxels());
        neighbor_propagation
            .iter()
            .fold(
                HashMap::<ChunkSide, Vec<_>>::new(),
                |mut map, &NeighborLightPropagation { side, voxel, .. }| {
                    map.entry(side).or_default().push(voxel);
                    map
                },
            )
            .into_iter()
            .for_each(|(_side, voxels)| {
                assert_eq!(voxels.len(), chunk::X_AXIS_SIZE * chunk::Y_AXIS_SIZE);
            });

        neighbor_propagation.into_iter().for_each(
            |NeighborLightPropagation {
                 voxel,
                 ty,
                 intensity,
                 ..
             }| {
                assert_eq!(
                    ty,
                    LightTy::Natural,
                    "Only natural light should be propagated"
                );
                assert_eq!(
                    intensity,
                    voxel::Light::MAX_NATURAL_INTENSITY - 1,
                    "Non-downwards propagation should reduce natural intensity"
                );
                assert!(
                    chunk::is_at_edge(voxel),
                    "All voxels propagated should be at boundry"
                );
            },
        );
    }

    #[test]
    fn propagate_to_neighborhood() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        chunk::top_voxels().for_each(|voxel| {
            light.set_type(voxel, LightTy::Natural, voxel::Light::MAX_NATURAL_INTENSITY);
        });

        let left_wall = (0..=chunk::Z_END)
            .flat_map(|z| (0..=chunk::Y_END).map(move |y| IVec3::new(0, y, z)))
            .collect::<Vec<_>>();
        left_wall.iter().for_each(|&v| kind.set(v, 1.into()));

        let neighbor_propagation =
            propagate(&kind, &mut light, LightTy::Natural, chunk::top_voxels());

        neighbor_propagation
            .into_iter()
            .for_each(|NeighborLightPropagation { side, .. }| {
                assert_ne!(
                    side,
                    chunk::ChunkSide::Left,
                    "No light should be propagated to left"
                );
            });
    }

    #[test]
    fn gather_neighborhood_light() {
        let chunk = Chunk::default();
        let voxel = Voxel::new(10, 10, 10);
        let kind = ChunkStorage::<voxel::Kind>::default();
        let mut light = ChunkStorage::<voxel::Light>::default();

        let mut i = 0;
        for y in -1..=1 {
            for z in -1..=1 {
                for x in -1..=1 {
                    light.set(voxel + Voxel::new(x, y, z), voxel::Light::natural(i));
                    i += 1
                }
            }
        }

        let get_kind = |_| -> _ { Some(&kind) };
        let get_light = |_| -> _ { Some(&light) };

        let neighbors = super::gather_neighborhood_light(chunk, voxel, get_kind, get_light);

        let mut i = 0;
        for y in -1..=1 {
            for z in -1..=1 {
                for x in -1..=1 {
                    let neighbor = voxel + Voxel::new(x, y, z);

                    if neighbor == voxel {
                        continue;
                    }

                    assert_eq!(
                        light.get(neighbor).get_greater_intensity(),
                        neighbors[i].unwrap(),
                        "Failed at {neighbor} [{i}]"
                    );
                    i += 1
                }
            }
        }
    }

    #[test]
    fn neighbor_lookup_table() {
        let mut count = vec![0; NEIGHBOR_COUNT];

        let corners = [0usize, 2, 19, 17, 6, 8, 25, 23];

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
