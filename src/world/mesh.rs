use std::collections::HashSet;

use bevy::math::IVec3;

use crate::world::storage::voxel;

use super::storage::{
    chunk::{self, Chunk},
    voxel::VoxelFace,
};

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

pub const VERTICES: [[f32; 3]; 8] = [
    [0.0, 0.0, 0.0], //v0
    [1.0, 0.0, 0.0], //v1
    [1.0, 1.0, 0.0], //v2
    [0.0, 1.0, 0.0], //v3
    [0.0, 0.0, 1.0], //v4
    [1.0, 0.0, 1.0], //v5
    [1.0, 1.0, 1.0], //v6
    [0.0, 1.0, 1.0], //v7
];

pub const VERTICES_INDICES: [[usize; 4]; 6] = [
    [1, 2, 6, 5], //RIGHT
    [0, 4, 7, 3], //LEFT
    [3, 7, 6, 2], //UP
    [0, 1, 5, 4], //DOWN
    [4, 5, 6, 7], //FRONT
    [0, 3, 2, 1], //BACK
];

pub fn compute_indices(vertex_count: usize) -> Vec<u32> {
    // Each 4 vertex is a voxel face and each voxel face has 6 indices, so we can multiply the vertex count by 1.5
    let index_count = (vertex_count as f32 * 1.5) as usize;

    let mut res = vec![0; index_count];
    let mut i = 0u32;

    while i < vertex_count as u32 {
        res.push(i);
        res.push(i + 1);
        res.push(i + 2);

        res.push(i + 2);
        res.push(i + 3);
        res.push(i);

        i += 4;
    }

    res
}

pub fn merge_faces(
    occlusion: &[voxel::FacesOcclusion; chunk::BUFFER_SIZE],
    chunk: &Chunk,
) -> Vec<VoxelFace> {
    fn should_skip_voxel(
        merged: &[HashSet<IVec3>; voxel::SIDE_COUNT],
        voxel: IVec3,
        side: voxel::Side,
        chunk: &Chunk,
        occlusion: &[voxel::FacesOcclusion; chunk::BUFFER_SIZE],
    ) -> bool {
        !chunk::is_within_bounds(voxel)
            || merged[side as usize].contains(&voxel)
            || occlusion[chunk::to_index(voxel)][side as usize]
            || chunk.get_kind(voxel) == 0
    }

    fn find_furthest_eq_voxel(
        begin: IVec3,
        step: IVec3,
        merged: &[HashSet<IVec3>; voxel::SIDE_COUNT],
        side: voxel::Side,
        chunk: &Chunk,
        occlusion: &[voxel::FacesOcclusion; chunk::BUFFER_SIZE],
    ) -> IVec3 {
        let mut next_voxel = begin + step;
        while chunk::is_within_bounds(next_voxel) {
            if should_skip_voxel(&merged, next_voxel, side, chunk, occlusion) {
                break;
            } else {
                next_voxel += step;
            }
        }
        next_voxel -= step;

        next_voxel
    }

    let mut faces_vertices = vec![];
    let mut merged = [
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
        HashSet::new(),
    ];

    for y in 0..chunk::AXIS_SIZE as i32 {
        for x in 0..chunk::AXIS_SIZE as i32 {
            for z in 0..chunk::AXIS_SIZE as i32 {
                let voxel = (x, y, z).into();
                for side in voxel::SIDES {
                    if side != voxel::Side::Up {
                        continue;
                    }

                    if should_skip_voxel(&merged, voxel, side, chunk, occlusion) {
                        continue;
                    }

                    // Finds the furthest equal voxel on current axis
                    let v1 = voxel;
                    let v2 = find_furthest_eq_voxel(
                        voxel,
                        (0, 0, 1).into(),
                        &merged,
                        side,
                        chunk,
                        occlusion,
                    );

                    let step = IVec3::new(1, 0, 0);
                    let mut v3 = v2 + step;
                    let mut tmp = v1 + step;
                    while !should_skip_voxel(&merged, tmp, side, chunk, occlusion) {
                        let furthest = find_furthest_eq_voxel(
                            tmp,
                            (0, 0, 1).into(),
                            &merged,
                            side,
                            chunk,
                            occlusion,
                        );

                        if furthest == v3 {
                            v3 += step;
                            tmp += step;
                        } else {
                            break;
                        }
                    }

                    v3 -= step;
                    let v4 = v1 + (v3 - v2);

                    for mx in v1.x..=v3.x {
                        for my in v1.y..=v3.y {
                            for mz in v1.z..=v3.z {
                                merged[side as usize].insert((mx, my, mz).into());
                            }
                        }
                    }

                    faces_vertices.push(VoxelFace {
                        vertices: [v1, v2, v3, v4],
                        side,
                    })
                }
            }
        }
    }

    faces_vertices
}

fn merge_up_faces(
    faces_vertices: &mut Vec<VoxelFace>,
    occlusion: &[voxel::FacesOcclusion; chunk::BUFFER_SIZE],
    chunk: &Chunk,
) {
    let side = voxel::Side::Up;

    let mut merged = HashSet::new();

    for x in 0..chunk::AXIS_SIZE as i32 {
        for z in 0..chunk::AXIS_SIZE as i32 {
            for y in 0..chunk::AXIS_SIZE as i32 {
                let voxel = IVec3::new(x, y, z);

                if chunk.get_kind(voxel) == 0 || occlusion[chunk::to_index(voxel)][side as usize] {
                    continue;
                }

                if merged.contains(&voxel) {
                    continue;
                }

                let mut end_z = z + 1;

                while end_z < chunk::AXIS_SIZE as i32 {
                    let next = (x, y, end_z).into();
                    let next_index = chunk::to_index(next);

                    if merged.contains(&next)
                        || occlusion[next_index][side as usize]
                        || chunk.get_kind(next) == 0
                    {
                        break;
                    } else {
                        end_z += 1;
                    }
                }

                end_z -= 1;

                let mut end_x = x + 1;
                'outer: while end_x < chunk::AXIS_SIZE as i32 {
                    for tmp_z in z..=end_z {
                        let next = (end_x, y, tmp_z).into();
                        let next_index = chunk::to_index(next);

                        if merged.contains(&next)
                            || occlusion[next_index][side as usize]
                            || chunk.get_kind(next) == 0
                        {
                            break 'outer;
                        }
                    }

                    end_x += 1;
                }

                end_x -= 1;

                for mx in x..=end_x {
                    for mz in z..=end_z {
                        merged.insert(IVec3::new(mx, y, mz));
                    }
                }

                let voxel_face = VoxelFace {
                    vertices: [
                        (x, y, z).into(),
                        (x, y, end_z).into(),
                        (end_x, y, end_z).into(),
                        (end_x, y, z).into(),
                    ],
                    side,
                };

                faces_vertices.push(voxel_face);
            }
        }
    }
}

fn merge_right_faces(
    faces_vertices: &mut Vec<VoxelFace>,
    occlusion: &[voxel::FacesOcclusion; chunk::BUFFER_SIZE],
    chunk: &Chunk,
) {
    let side = voxel::Side::Right;

    let mut merged = HashSet::new();

    for x in 0..chunk::AXIS_SIZE as i32 {
        for z in 0..chunk::AXIS_SIZE as i32 {
            for y in 0..chunk::AXIS_SIZE as i32 {
                let voxel = IVec3::new(x, y, z);

                if chunk.get_kind(voxel) == 0 || occlusion[chunk::to_index(voxel)][side as usize] {
                    continue;
                }

                if merged.contains(&voxel) {
                    continue;
                }

                let mut end_z = z + 1;

                while end_z < chunk::AXIS_SIZE as i32 {
                    let next = (x, y, end_z).into();
                    let next_index = chunk::to_index(next);

                    if merged.contains(&next)
                        || occlusion[next_index][side as usize]
                        || chunk.get_kind(next) == 0
                    {
                        break;
                    } else {
                        end_z += 1;
                    }
                }

                end_z -= 1;

                let mut end_y = y + 1;
                'outer: while end_y < chunk::AXIS_SIZE as i32 {
                    for tmp_z in z..=end_z {
                        let next = (x, end_y, tmp_z).into();
                        let next_index = chunk::to_index(next);

                        if merged.contains(&next)
                            || occlusion[next_index][side as usize]
                            || chunk.get_kind(next) == 0
                        {
                            break 'outer;
                        }
                    }

                    end_y += 1;
                }

                end_y -= 1;

                for my in y..=end_y {
                    for mz in z..=end_z {
                        merged.insert(IVec3::new(x, my, mz));
                    }
                }

                let voxel_face = VoxelFace {
                    vertices: [
                        (x, y, z).into(),
                        (x, end_y, z).into(),
                        (x, end_y, end_z).into(),
                        (x, y, end_z).into(),
                    ],
                    side,
                };

                faces_vertices.push(voxel_face);
            }
        }
    }
}

/*

RIGHT: Y, Z (1, 2) +
LEFT: Z, Y (2, 1) -
UP: X, Z (0, 2) +
DOWN: Z, X (2, 0) -
FRONT: X, Y (0, 1) +
BACK: Y, X (1, 0) -


*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_faces() {
        let mut chunk = Chunk::default();
        let mut occlusion = [voxel::FacesOcclusion::default(); chunk::BUFFER_SIZE];

        assert!(
            super::merge_faces(&occlusion, &chunk).is_empty(),
            "No face should be returned on empty chunk"
        );

        chunk.set_kind((0, 0, 0).into(), 1);
        occlusion.fill([true; voxel::SIDE_COUNT]);

        assert!(
            super::merge_faces(&occlusion, &chunk).is_empty(),
            "No face should be returned on full occlusion"
        );

        occlusion.fill(voxel::FacesOcclusion::default());
        for x in 0..10 {
            for z in 0..10 {
                chunk.set_kind((x, 0, z).into(), 1);
                occlusion[chunk::to_index((x, 0, z).into())][voxel::Side::Up as usize] = false;
            }
        }

        let faces = super::merge_faces(&occlusion, &chunk);
        let mut expected = vec![VoxelFace {
            side: voxel::Side::Up,
            vertices: [
                (0, 0, 0).into(),
                (0, 0, 9).into(),
                (9, 0, 9).into(),
                (9, 0, 0).into(),
            ],
        }];

        assert_eq!(faces, expected);

        for x in 0..10 {
            for z in 0..10 {
                chunk.set_kind((x, 1, z).into(), 1);
                occlusion[chunk::to_index((x, 1, z).into())][voxel::Side::Up as usize] = false;
            }
        }

        let faces = super::merge_faces(&occlusion, &chunk);
        expected.extend(vec![VoxelFace {
            side: voxel::Side::Up,
            vertices: [
                (0, 1, 0).into(),
                (0, 1, 9).into(),
                (9, 1, 9).into(),
                (9, 1, 0).into(),
            ],
        }]);

        assert_eq!(faces, expected);

        chunk.set_kind((15, 15, 15).into(), 2);
        occlusion[chunk::to_index((15, 15, 15).into())][voxel::Side::Up as usize] = false;

        let faces = super::merge_faces(&occlusion, &chunk);
        expected.extend(vec![VoxelFace {
            side: voxel::Side::Up,
            vertices: [
                (15, 15, 15).into(),
                (15, 15, 15).into(),
                (15, 15, 15).into(),
                (15, 15, 15).into(),
            ],
        }]);

        assert_eq!(faces, expected);

        let mut chunk = Chunk::default();
        let mut occlusion = [[true; voxel::SIDE_COUNT]; chunk::BUFFER_SIZE];

        // Set a square
        for x in 10..15 {
            for z in 10..15 {
                chunk.set_kind((x, 1, z).into(), 1);
                occlusion[chunk::to_index((x, 1, z).into())][voxel::Side::Up as usize] = false;
            }
        }

        // Place a hole on the middle
        chunk.set_kind((12, 1, 12).into(), 0);
        occlusion[chunk::to_index((12, 1, 12).into())][voxel::Side::Up as usize] = false;

        let side = voxel::Side::Up;
        let faces = super::merge_faces(&occlusion, &chunk);
        assert_eq!(
            faces,
            vec![
                VoxelFace {
                    side,
                    vertices: [
                        (10, 1, 10).into(),
                        (10, 1, 14).into(),
                        (11, 1, 14).into(),
                        (11, 1, 10).into(),
                    ],
                },
                VoxelFace {
                    side,
                    vertices: [
                        (12, 1, 10).into(),
                        (12, 1, 11).into(),
                        (12, 1, 11).into(),
                        (12, 1, 10).into(),
                    ]
                },
                VoxelFace {
                    side,
                    vertices: [
                        (12, 1, 13).into(),
                        (12, 1, 14).into(),
                        (14, 1, 14).into(),
                        (14, 1, 13).into(),
                    ]
                },
                VoxelFace {
                    side,
                    vertices: [
                        (13, 1, 10).into(),
                        (13, 1, 12).into(),
                        (14, 1, 12).into(),
                        (14, 1, 10).into(),
                    ]
                },
            ]
        );

        let mut chunk = Chunk::default();
        let mut occlusion = [[true; voxel::SIDE_COUNT]; chunk::BUFFER_SIZE];

        // for x in 0..2 {
        //     for z in 0..chunk::AXIS_SIZE as i32 {
        //         chunk.set_kind((x, 1, z).into(), 1);
        //         occlusion[chunk::to_index((x, 1, z).into())][voxel::Side::Up as usize] = false;
        //     }
        // }

        chunk.set_kind((1, 0, 0).into(), 1);
        occlusion[chunk::to_index((1, 0, 0).into())][voxel::Side::Up as usize] = false;

        chunk.set_kind((2, 0, 0).into(), 1);
        occlusion[chunk::to_index((2, 0, 0).into())][voxel::Side::Up as usize] = false;

        let side = voxel::Side::Up;
        let faces = super::merge_faces(&occlusion, &chunk);
        assert_eq!(
            faces,
            vec![VoxelFace {
                side,
                vertices: [
                    (1, 0, 0).into(),
                    (1, 0, 0).into(),
                    (2, 0, 0).into(),
                    (2, 0, 0).into(),
                ],
            }],
        );
    }
}
