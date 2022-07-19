use bevy::math::IVec3;

use crate::world::{query, storage::voxel};

use super::{
    storage::{
        chunk::{self, Chunk, ChunkKind},
        voxel::VoxelFace,
    },
    terraformation::ChunkFacesOcclusion,
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
    [7, 6, 2, 3], //UP
    [0, 1, 5, 4], //DOWN
    [4, 5, 6, 7], //FRONT
    [1, 0, 3, 2], //BACK
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

pub fn merge_faces(occlusion: &ChunkFacesOcclusion, chunk: &Chunk) -> Vec<VoxelFace> {
    #[inline]
    fn should_skip_voxel(
        merged: &Vec<usize>,
        voxel: IVec3,
        side: voxel::Side,
        kinds: &ChunkKind,
        occlusion: &ChunkFacesOcclusion,
    ) -> bool {
        // perf_fn_scope!();
        !chunk::is_within_bounds(voxel)
            || kinds.get(voxel).is_empty()
            || merged[chunk::to_index(voxel)] == 1
            || occlusion.get(voxel).is_occluded(side)
    }

    #[inline]
    fn find_furthest_eq_voxel(
        begin: IVec3,
        step: IVec3,
        merged: &Vec<usize>,
        side: voxel::Side,
        kinds: &ChunkKind,
        occlusion: &ChunkFacesOcclusion,
    ) -> IVec3 {
        // perf_fn_scope!();

        let kind = kinds.get(begin);
        let mut next_voxel = begin + step;

        while !should_skip_voxel(merged, next_voxel, side, kinds, occlusion)
            && kinds.get(next_voxel) == kind
        {
            next_voxel += step;
        }

        next_voxel -= step;

        next_voxel
    }

    let mut _perf = perf_fn!();
    let mut faces_vertices = vec![];

    let side_axis = [
        (-IVec3::Z, IVec3::Y), //RIGHT
        (IVec3::Z, IVec3::Y),  //LEFT
        (IVec3::X, -IVec3::Z),  //UP
        (IVec3::X, IVec3::Z),  //DOWN
        (IVec3::X, IVec3::Y),  //FRONT
        (-IVec3::X, IVec3::Y),  //BACK
    ];

    let kinds = &chunk.kinds;

    for side in voxel::SIDES {
        let axis = side_axis[side as usize];
        let mut merged = vec![0; chunk::BUFFER_SIZE];

        for voxel in chunk::voxels() {
            if should_skip_voxel(&merged, voxel, side, kinds, occlusion) {
                continue;
            }

            perf_scope!(_perf);

            let kind = kinds.get(voxel);

            // Finds the furthest equal voxel on current axis
            let v1 = voxel;
            let v2 = find_furthest_eq_voxel(voxel, axis.0, &merged, side, kinds, occlusion);

            let step = axis.1;
            let mut v3 = v2 + step;
            let mut tmp = v1 + step;
            while !should_skip_voxel(&merged, tmp, side, kinds, occlusion) {
                let furthest = find_furthest_eq_voxel(tmp, axis.0, &merged, side, kinds, occlusion);

                if furthest == v3 {
                    v3 += step;
                    tmp += step;
                } else {
                    break;
                }
            }

            v3 -= step;
            let v4 = v1 + (v3 - v2);

            for voxel in query::range_inclusive(v1, v3) {
                merged[chunk::to_index(voxel)] = 1;
            }

            faces_vertices.push(VoxelFace {
                vertices: [v1, v2, v3, v4],
                side,
                kind,
            })
        }
    }

    faces_vertices
}
