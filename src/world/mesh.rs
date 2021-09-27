use std::collections::HashSet;

use bevy::math::IVec3;

use crate::world::{query, storage::voxel};

use super::{
    pipeline::ChunkFacesOcclusion,
    storage::{
        chunk::{self, ChunkKind},
        voxel::VoxelFace,
    },
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

pub fn merge_faces(occlusion: &ChunkFacesOcclusion, chunk: &ChunkKind) -> Vec<VoxelFace> {
    fn should_skip_voxel(
        merged: &HashSet<IVec3>,
        voxel: IVec3,
        side: voxel::Side,
        chunk: &ChunkKind,
        occlusion: &ChunkFacesOcclusion,
    ) -> bool {
        // perf_fn_scope!();
        !chunk::is_within_bounds(voxel)
            || chunk.get(voxel).is_empty()
            || merged.contains(&voxel)
            || occlusion.get(voxel).is_occluded(side)
    }

    fn find_furthest_eq_voxel(
        begin: IVec3,
        step: IVec3,
        merged: &HashSet<IVec3>,
        side: voxel::Side,
        chunk: &ChunkKind,
        occlusion: &ChunkFacesOcclusion,
    ) -> IVec3 {
        // perf_fn_scope!();
        let mut next_voxel = begin + step;

        while !should_skip_voxel(merged, next_voxel, side, chunk, occlusion) {
            next_voxel += step;
        }
        next_voxel -= step;

        next_voxel
    }

    let mut _perf = perf_fn!();
    let mut faces_vertices = vec![];

    let side_axis = [
        (IVec3::Y, IVec3::Z),
        (IVec3::Z, IVec3::Y),
        (IVec3::Z, IVec3::X),
        (IVec3::X, IVec3::Z),
        (IVec3::X, IVec3::Y),
        (IVec3::Y, IVec3::X),
    ];

    for side in voxel::SIDES {
        let axis = side_axis[side as usize];
        let mut merged = HashSet::default();

        for voxel in chunk::voxels() {
            if should_skip_voxel(&merged, voxel, side, chunk, occlusion) {
                continue;
            }

            perf_scope!(_perf);

            // Finds the furthest equal voxel on current axis
            let v1 = voxel;
            let v2 = find_furthest_eq_voxel(voxel, axis.0, &merged, side, chunk, occlusion);

            let step = axis.1;
            let mut v3 = v2 + step;
            let mut tmp = v1 + step;
            while !should_skip_voxel(&merged, tmp, side, chunk, occlusion) {
                let furthest = find_furthest_eq_voxel(tmp, axis.0, &merged, side, chunk, occlusion);

                if furthest == v3 {
                    v3 += step;
                    tmp += step;
                } else {
                    break;
                }
            }

            v3 -= step;
            let v4 = v1 + (v3 - v2);

            merged.extend(query::range_inclusive(v1, v3));

            faces_vertices.push(VoxelFace {
                vertices: [v1, v2, v3, v4],
                side,
            })
        }
    }

    faces_vertices
}
