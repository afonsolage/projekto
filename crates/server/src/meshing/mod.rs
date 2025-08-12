use bevy::math::{IVec3, Vec3};
use projekto_core::{
    chunk::{self, ChunkSide, ChunkStorage},
    math,
    voxel::{self, FacesOcclusion},
};

pub mod greedy;

// v3               v2
// +-----------+
// v7  / |      v6 / | +-----------+   |
// |   |       |   |
// |   +-------|---+
// | /  v0     | /  v1
// +-----------+
// v4           v5
//
// Y
// |
// +---X
// /
// Z

pub const VERTICES: [[f32; 3]; 8] = [
    [0.0, 0.0, 0.0], // v0
    [1.0, 0.0, 0.0], // v1
    [1.0, 1.0, 0.0], // v2
    [0.0, 1.0, 0.0], // v3
    [0.0, 0.0, 1.0], // v4
    [1.0, 0.0, 1.0], // v5
    [1.0, 1.0, 1.0], // v6
    [0.0, 1.0, 1.0], // v7
];

pub const VERTICES_INDICES: [[usize; 4]; 6] = [
    [5, 1, 2, 6], // RIGHT
    [0, 4, 7, 3], // LEFT
    [7, 6, 2, 3], // UP
    [0, 1, 5, 4], // DOWN
    [4, 5, 6, 7], // FRONT
    [1, 0, 3, 2], // BACK
];

/// Generates vertices data from a given [`voxel::Face`] list.
///
/// All generated indices will be relative to a triangle list.
///
/// **Returns** a list of generated [`voxel::Vertex`].
pub fn generate_vertices(faces: &[voxel::Face]) -> Vec<voxel::Vertex> {
    const VERTICES_ESTIMATION: usize = (chunk::BUFFER_SIZE * voxel::SIDE_COUNT * 6) / 100;
    const LIGHT_FRACTION: f32 = (voxel::Light::MAX_NATURAL_INTENSITY as f32).recip();

    #[inline]
    fn calc_tile_size(min: Vec3, max: Vec3) -> f32 {
        (min.x - max.x).abs() + (min.y - max.y).abs() + (min.z - max.z).abs()
    }

    let mut vertices = Vec::with_capacity(VERTICES_ESTIMATION);
    let kinds_descs = voxel::KindsDescs::get();

    let mut faces_vertices = [Vec3::ZERO; 4];

    for face in faces {
        let normal = face.side.normal();

        let face_desc = kinds_descs.get_face_desc(face);
        let tile_texture_size = (kinds_descs.count_tiles() as f32).recip();
        let tile_coord_start = face_desc.offset.as_vec2() * tile_texture_size;

        for (i, v) in face.vertices.iter().enumerate() {
            let base_vertex_idx = VERTICES_INDICES[face.side as usize][i];
            let base_vertex: Vec3 = VERTICES[base_vertex_idx].into();

            faces_vertices[i] = base_vertex + v.as_vec3();
        }

        debug_assert!(
            faces_vertices.len() == 4,
            "Each face should have 4 vertices"
        );

        let x_tile = calc_tile_size(faces_vertices[0], faces_vertices[1]) * tile_texture_size;
        let y_tile = calc_tile_size(faces_vertices[0], faces_vertices[3]) * tile_texture_size;

        let tile_uv = [
            (0.0, y_tile).into(),
            (x_tile, y_tile).into(),
            (x_tile, 0.0).into(),
            (0.0, 0.0).into(),
        ];

        for (i, v) in faces_vertices.iter().copied().enumerate() {
            vertices.push(voxel::Vertex {
                position: v,
                normal,
                uv: tile_uv[i],
                tile_coord_start,
                light: Vec3::splat(face.light[i] * LIGHT_FRACTION),
            });
        }
    }

    debug_assert!(!vertices.is_empty());
    vertices
}

pub fn faces_occlusion(
    kind: &ChunkStorage<voxel::Kind>,
    faces_occlusion: &mut ChunkStorage<voxel::FacesOcclusion>,
    neighboorhood: &[Option<&ChunkStorage<voxel::Kind>>; chunk::SIDE_COUNT],
) {
    const DIM: IVec3 = IVec3::new(
        chunk::X_AXIS_SIZE as i32,
        chunk::Y_AXIS_SIZE as i32,
        chunk::Z_AXIS_SIZE as i32,
    );

    for x in 0..chunk::X_AXIS_SIZE as i32 {
        for z in 0..chunk::Z_AXIS_SIZE as i32 {
            for y in 0..chunk::Y_AXIS_SIZE as i32 {
                let voxel = IVec3::new(x, y, z);

                if kind.get(voxel).is_none() {
                    faces_occlusion.set(voxel, voxel::FacesOcclusion::fully_occluded());
                } else {
                    let faces =
                        voxel::SIDES
                            .iter()
                            .fold(FacesOcclusion::default(), |mut faces, &side| {
                                let neighbor = voxel + side.dir();

                                let neighbor_kind = if chunk::is_inside(neighbor) {
                                    kind.get(neighbor)
                                } else if let Some(chunk_side) = ChunkSide::from_voxel_side(side)
                                    && let Some(neighbor_kind) = neighboorhood[chunk_side as usize]
                                {
                                    let neighbor_chunk_voxel = math::euclid_rem(neighbor, DIM);
                                    neighbor_kind.get(neighbor_chunk_voxel)
                                } else {
                                    return faces;
                                };

                                faces.set(side, !neighbor_kind.is_none());
                                faces
                            });

                    faces_occlusion.set(voxel, faces);
                }
            }
        }
    }
}

pub fn generate_faces(
    kind: &ChunkStorage<voxel::Kind>,
    occlusion: &ChunkStorage<voxel::FacesOcclusion>,
    soft_light: &ChunkStorage<voxel::FacesSoftLight>,
) -> Vec<voxel::Face> {
    const FACES_ESTIMATION: usize = (chunk::BUFFER_SIZE * voxel::SIDE_COUNT) / 2;

    let mut faces_vertices = Vec::with_capacity(FACES_ESTIMATION);

    for voxel in chunk::voxels() {
        let kind = kind.get(voxel);
        if kind.is_none() {
            continue;
        }

        let occlusion = occlusion.get(voxel);
        let voxel_soft_light = soft_light.get(voxel);

        for side in voxel::SIDES {
            if occlusion.is_occluded(side) {
                continue;
            }

            let (v1, v2, v3, v4) = (voxel, voxel, voxel, voxel);
            faces_vertices.push(voxel::Face {
                vertices: [v1, v2, v3, v4],
                side,
                kind,
                light: voxel_soft_light.get(side),
            });
        }
    }

    faces_vertices
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn faces_occlusion_empty_chunk() {
        let kind = Default::default();
        let mut faces_occlusion = Default::default();
        let neighborhood = [None; chunk::SIDE_COUNT];

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        assert!(
            faces_occlusion.all(|occ| occ.is_fully_occluded()),
            "Should be fully occluded in an empty chunk"
        );
    }

    #[test]
    fn faces_occlusion_opaque_voxel() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut faces_occlusion = Default::default();
        let neighborhood = [None; chunk::SIDE_COUNT];

        kind.set([0, 0, 0].into(), 1.into());

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        let occ = faces_occlusion.get([0, 0, 0].into());

        voxel::SIDES.iter().for_each(|&side| {
            assert!(!occ.is_occluded(side), "No side should be occluded");
        });
    }

    #[test]
    fn faces_occlusion_neighbor() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut neighbor_kind = ChunkStorage::<voxel::Kind>::default();
        let mut faces_occlusion = Default::default();
        let mut neighborhood = [None; chunk::SIDE_COUNT];

        kind.set([0, 0, 0].into(), 1.into());
        neighbor_kind.set([chunk::X_END, 0, 0].into(), 1.into());
        neighborhood[voxel::Side::Left as usize] = Some(&neighbor_kind);

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        let occ = faces_occlusion.get([0, 0, 0].into());

        voxel::SIDES.iter().for_each(|&side| {
            if side == voxel::Side::Left {
                assert!(occ.is_occluded(side), "Left side should be occluded");
            } else {
                assert!(
                    !occ.is_occluded(side),
                    "All other sides should not be occluded"
                );
            }
        });
    }
}
