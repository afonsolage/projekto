use bevy::math::{IVec3, Vec3};

use crate::world::{
    query,
    storage::voxel::{self, FacesOcclusion},
};

use crate::world::{
    storage::{
        chunk::{self, Chunk, ChunkKind, ChunkNeighborhood},
        voxel::{KindsDescs, VoxelFace, VoxelVertex},
        VoxWorld,
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

/**
 Computes indices of a triangle list mesh.

 This function assumes 4 vertices per face, 3 indices per triangles and all vertices are placed in CCW order.
 
 It generates indices in the following order: _*0 1 2 2 3 0*_ where 0 is the first vertice and 3 is the last one

 **Returns** a list of indices in the CCW order
*/
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

/**
  Recompute chunk kind neighborhood and vertices.

  This function should be called whenever the chunk has changed and needs to update it's internal state.

  **Returns** true of the chunk was recomputed, false otherwise.
 */
pub fn recompute_chunk(world: &mut VoxWorld, kinds_descs: &KindsDescs, local: IVec3) -> bool {
    let neighborhood = gather_kind_neighborhood(world, local);

    if let Some(chunk) = world.get_mut(local) {
        let occlusion = faces_occlusion(&chunk.kinds);
        if occlusion.is_fully_occluded() {
            chunk.vertices = vec![]
        } else {
            let faces = merge_faces(occlusion, chunk);
            chunk.vertices = generate_vertices(faces, kinds_descs);
        }

        chunk.kinds.neighborhood = neighborhood;

        true
    } else {
        false
    }
}

/**
  Merge all faces which have the same voxel properties, like kind, lighting, AO and so on.

  The basic logic of function was based on [Greedy Mesh](https://0fps.net/2012/06/30/meshing-in-a-minecraft-game/).
  It was heavy modified to use a less mathematical and more logic approach (Yeah I don't understood those aliens letters).

  This function is very CPU intense so it should be run in a separated thread to avoid FPS drops.

  **Returns** a list of merged [`VoxelFace`] 
 */
fn merge_faces(occlusion: ChunkFacesOcclusion, chunk: &Chunk) -> Vec<VoxelFace> {
    // TODO: I feel that it is still possible to reorganize this function to have better readability, but since this is a heavy function, I'll keep it as it is for now

    /**
      Checks if voxel is out of bounds, or is empty or is already merged or is fully occluded.
    */
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

    // Which direction the algorithm will walk in order to merge faces.
    // Pretty sure it's possible to calculate this using some dot and normal dark magic, but I'm too dumb to figure it out.
    let side_walk_axis = [
        (-IVec3::Z, IVec3::Y), //RIGHT
        (IVec3::Z, IVec3::Y),  //LEFT
        (IVec3::X, -IVec3::Z), //UP
        (IVec3::X, IVec3::Z),  //DOWN
        (IVec3::X, IVec3::Y),  //FRONT
        (-IVec3::X, IVec3::Y), //BACK
    ];

    let kinds = &chunk.kinds;

    for side in voxel::SIDES {
        let walk_axis = side_walk_axis[side as usize];
        let mut merged = vec![0; chunk::BUFFER_SIZE];

        for voxel in chunk::voxels() {
            if should_skip_voxel(&merged, voxel, side, kinds, &occlusion) {
                continue;
            }

            perf_scope!(_perf);

            let kind = kinds.get(voxel);

            // Finds the furthest equal voxel on current axis
            let v1 = voxel;
            let v2 = find_furthest_eq_voxel(voxel, walk_axis.0, &merged, side, kinds, &occlusion);

            let step = walk_axis.1;
            let mut v3 = v2 + step;
            let mut tmp = v1 + step;

            while !should_skip_voxel(&merged, tmp, side, kinds, &occlusion)
                && kinds.get(tmp) == kind
            {
                let furthest =
                    find_furthest_eq_voxel(tmp, walk_axis.0, &merged, side, kinds, &occlusion);

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

/**
Computes the faces occlusion data of the given [`ChunkKind`]

**Returns** computed [`ChunkFacesOcclusion`]
*/
fn faces_occlusion(chunk: &ChunkKind) -> ChunkFacesOcclusion {
    perf_fn_scope!();

    let mut occlusion = ChunkFacesOcclusion::default();
    for voxel in chunk::voxels() {
        let mut voxel_faces = FacesOcclusion::default();

        if chunk.get(voxel).is_empty() {
            voxel_faces.set_all(true);
        } else {
            for side in voxel::SIDES {
                let dir = side.dir();
                let neighbor_pos = voxel + dir;

                let neighbor_kind = if !chunk::is_within_bounds(neighbor_pos) {
                    let (_, next_chunk_voxel) = chunk::overlap_voxel(neighbor_pos);

                    match chunk.neighborhood.get(side, next_chunk_voxel) {
                        Some(k) => k,
                        None => continue,
                    }
                } else {
                    chunk.get(neighbor_pos)
                };

                voxel_faces.set(side, !neighbor_kind.is_empty());
            }
        }

        occlusion.set(voxel, voxel_faces);
    }

    occlusion
}

/**
Generates vertices data from a given [`VoxelFace`] list.

All generated indices will be relative to a triangle list.

**Returns** a list of generated [`VoxelVertex`].
*/
fn generate_vertices(faces: Vec<VoxelFace>, kinds_descs: &KindsDescs) -> Vec<VoxelVertex> {
    perf_fn_scope!();

    let mut vertices = vec![];
    let tile_texture_size = 1.0 / kinds_descs.count_tiles() as f32;

    for face in faces {
        let normal = face.side.normal();

        let face_desc = kinds_descs.get_face_desc(&face);
        let tile_coord_start = face_desc.offset.as_vec2() * tile_texture_size;

        let faces_vertices = face
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let base_vertex_idx = VERTICES_INDICES[face.side as usize][i];
                let base_vertex: Vec3 = VERTICES[base_vertex_idx].into();

                base_vertex + v.as_vec3()
            })
            .collect::<Vec<_>>();

        debug_assert!(
            faces_vertices.len() == 4,
            "Each face should have 4 vertices"
        );

        fn calc_tile_size(min: Vec3, max: Vec3) -> f32 {
            (min.x - max.x).abs() + (min.y - max.y).abs() + (min.z - max.z).abs()
        }

        let x_tile = calc_tile_size(faces_vertices[0], faces_vertices[1]) * tile_texture_size;
        let y_tile = calc_tile_size(faces_vertices[0], faces_vertices[3]) * tile_texture_size;

        let tile_uv = [
            (0.0, y_tile).into(),
            (x_tile, y_tile).into(),
            (x_tile, 0.0).into(),
            (0.0, 0.0).into(),
        ];

        for (i, v) in faces_vertices.into_iter().enumerate() {
            vertices.push(VoxelVertex {
                position: v,
                normal,
                uv: tile_uv[i],
                tile_coord_start,
            });
        }
    }

    debug_assert!(!vertices.is_empty());
    vertices
}

/**
Updates the [`ChunkNeighborhood`] of a given chunk local.
This function updates any neighborhood data needed by chunk.

Currently it only updates kind neighborhood data, but in the future, it may update light and other relevant data.
*/
fn gather_kind_neighborhood(world: &VoxWorld, local: IVec3) -> ChunkNeighborhood<voxel::Kind> {
    let mut neighborhood = ChunkNeighborhood::default();
    for side in voxel::SIDES {
        let dir = side.dir();
        let neighbor = local + dir;

        if let Some(neighbor_chunk) = world.get(neighbor) {
            neighborhood.set(side, &neighbor_chunk.kinds);
        }
    }
    neighborhood
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_occlusion_occlude_empty_chunk() {
        // Arrange
        let chunk = ChunkKind::default();

        // Act
        let occlusions = super::faces_occlusion(&chunk);

        // Assert
        assert!(
            occlusions.iter().all(|a| a.is_fully_occluded()),
            "A chunk full of empty-kind voxels should be fully occluded"
        );
    }

    #[test]
    fn faces_occlusion() {
        // Arrange
        let mut chunk = ChunkKind::default();

        // Top-Bottom occlusion
        chunk.set((1, 1, 1).into(), 1.into());
        chunk.set((1, 2, 1).into(), 1.into());

        // Full occluded voxel at (10, 10, 10)
        chunk.set((10, 10, 10).into(), 1.into());
        chunk.set((9, 10, 10).into(), 1.into());
        chunk.set((11, 10, 10).into(), 1.into());
        chunk.set((10, 9, 10).into(), 1.into());
        chunk.set((10, 11, 10).into(), 1.into());
        chunk.set((10, 10, 9).into(), 1.into());
        chunk.set((10, 10, 11).into(), 1.into());

        // Act
        let faces_occlusion = super::faces_occlusion(&chunk);

        // Assert
        let faces = faces_occlusion.get((1, 2, 1).into());

        assert_eq!(
            faces,
            [false, false, false, true, false, false].into(),
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.get((1, 1, 1).into());

        assert_eq!(
            faces,
            [false, false, true, false, false, false].into(),
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.get((10, 10, 10).into());

        assert_eq!(
            faces,
            [true; voxel::SIDE_COUNT].into(),
            "Voxel fully surrounded by another non-empty voxels should be fully occluded"
        );
    }

    #[test]
    fn update_neighborhood() {
        let mut world = VoxWorld::default();

        let center = (1, 1, 1).into();
        let mut chunk = Chunk::default();
        chunk.kinds.set_all(10.into());
        world.add(center, chunk);

        for side in voxel::SIDES {
            let dir = side.dir();
            let pos = center + dir;
            let mut chunk = Chunk::default();
            chunk.kinds.set_all((side as u16).into());
            world.add(pos, chunk);
        }

        let neighborhood = super::gather_kind_neighborhood(&mut world, center);
        let chunk = world.get_mut(center).unwrap();
        chunk.kinds.neighborhood = neighborhood;

        for side in voxel::SIDES {
            match side {
                voxel::Side::Right => {
                    for a in 0..chunk::Y_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (0, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Left => {
                    for a in 0..chunk::Y_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (chunk::X_END as i32, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Up => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, 0, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Down => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, chunk::Y_END as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Front => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Y_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, b as i32, 0).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Back => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Y_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, b as i32, chunk::Z_END as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn faces_occlusion_neighborhood() {
        let mut world = VoxWorld::default();

        let mut top = Chunk::default();
        top.kinds.set_all(2.into());

        let mut down = Chunk::default();
        down.kinds.set_all(3.into());

        let mut center = Chunk::default();
        center
            .kinds
            .set((0, chunk::Y_END as i32, 0).into(), 1.into());
        center.kinds.set((1, 0, 1).into(), 1.into());

        world.add((0, 1, 0).into(), top);
        world.add((0, 0, 0).into(), center);
        world.add((0, -1, 0).into(), down);

        let neighborhood = super::gather_kind_neighborhood(&mut world, (0, 0, 0).into());
        let center = world.get_mut((0, 0, 0).into()).unwrap();
        center.kinds.neighborhood = neighborhood;

        let faces_occlusion = super::faces_occlusion(&center.kinds);

        let faces = faces_occlusion.get((0, chunk::Y_END as i32, 0).into());
        assert_eq!(faces, [false, false, true, false, false, false].into());

        let faces = faces_occlusion.get((1, 0, 1).into());
        assert_eq!(faces, [false, false, false, true, false, false].into());
    }
}
