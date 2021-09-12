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
    [5, 1, 2, 6], //RIGHT
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
    let mut faces_vertices = vec![];

    let mut merged = HashSet::new();

    for x in 0..chunk::AXIS_SIZE as i32 {
        for z in 0..chunk::AXIS_SIZE as i32 {
            for y in 0..chunk::AXIS_SIZE as i32 {
                let voxel = IVec3::new(x, y, z);

                if chunk.get_kind(voxel) == 0 || occlusion[chunk::to_index(voxel)][2] {
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
                        || occlusion[next_index][2]
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
                            || occlusion[next_index][2]
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
                    side: voxel::Side::Up,
                };

                faces_vertices.push(voxel_face);
            }
        }
    }

    faces_vertices
}

/*

RIGHT: Y, Z (1, 2) +
LEFT: Z, Y (2, 1) -
UP: X, Z (0, 2) +
DOWN: Z, X (2, 0) -
FRONT: X, Y (0, 1) +
BACK: Y, X (1, 0) -


*/
