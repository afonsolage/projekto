use bevy::math::{IVec3, Vec3};
use projekto_core::{
    chunk::{self, ChunkSide, ChunkStorage},
    coords::{Chunk, ChunkVoxel},
    math,
    voxel::{self, FacesOcclusion},
};

pub mod greedy;
pub mod lod;

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
    const VERTICES_ESTIMATION: usize = (Chunk::BUFFER_SIZE * voxel::SIDE_COUNT * 6) / 100;
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
        let tint = Vec3::new(face_desc.color.0, face_desc.color.1, face_desc.color.2);

        for (i, v) in face.vertices.iter().enumerate() {
            let base_vertex_idx = VERTICES_INDICES[face.side as usize][i];
            let base_vertex: Vec3 = VERTICES[base_vertex_idx].into();

            faces_vertices[i] = base_vertex + Vec3::new(v.x as f32, v.y as f32, v.z as f32);
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
                light: Vec3::splat(face.light[i] * LIGHT_FRACTION) * tint,
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
    faces_occlusion_with_lod(
        kind,
        faces_occlusion,
        neighboorhood,
        &[None; chunk::SIDE_COUNT],
    );
}

/// Per-layer meshing coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerCoverage {
    /// No faces can be exposed by this layer.
    Skip,
    /// Only voxels on the listed chunk sides need to be checked.
    Boundary(u8),
    /// Every voxel in the layer needs to be checked.
    Full,
}

/// Classifies each layer by how much meshing work it needs.
///
/// Uniform air layers are skipped. Uniform solid layers with solid neighbors are skipped when
/// fully enclosed, or reduced to boundary-only work when a horizontal neighbor chunk is not
/// loaded. Boundary layers only need the 16x16 perimeter voxels; their interior faces are
/// occluded by solid chunks on every side.
pub fn layer_coverage(
    kind: &ChunkStorage<voxel::Kind>,
    neighborhood: &[Option<&ChunkStorage<voxel::Kind>>; chunk::SIDE_COUNT],
) -> [LayerCoverage; Chunk::Y_AXIS_SIZE] {
    let mut coverage = [LayerCoverage::Full; Chunk::Y_AXIS_SIZE];

    for (y, layer) in coverage.iter_mut().enumerate() {
        let Some(value) = kind.layer_uniform(y) else {
            continue;
        };

        if value.is_none() {
            *layer = LayerCoverage::Skip;
            continue;
        }

        let above = y + 1 < Chunk::Y_AXIS_SIZE
            && kind
                .layer_uniform(y + 1)
                .is_some_and(|value| !value.is_none());
        let below = y > 0
            && kind
                .layer_uniform(y - 1)
                .is_some_and(|value| !value.is_none());

        if !above || !below {
            continue;
        }

        let mut boundary = 0;

        for side in chunk::SIDES {
            let bit = 1 << side.index();
            match neighborhood[side.index()] {
                Some(neighbor) => {
                    if neighbor
                        .layer_uniform(y)
                        .is_none_or(|value| value.is_none())
                    {
                        boundary |= bit;
                    }
                }
                None => boundary |= bit,
            }
        }

        *layer = if boundary == 0 {
            LayerCoverage::Skip
        } else {
            LayerCoverage::Boundary(boundary)
        };
    }

    coverage
}

pub(crate) fn is_boundary_voxel(voxel: ChunkVoxel, boundary: u8) -> bool {
    (boundary & (1 << chunk::ChunkSide::Right.index()) != 0 && voxel.x == Chunk::X_END)
        || (boundary & (1 << chunk::ChunkSide::Left.index()) != 0 && voxel.x == 0)
        || (boundary & (1 << chunk::ChunkSide::Front.index()) != 0 && voxel.z == Chunk::Z_END)
        || (boundary & (1 << chunk::ChunkSide::Back.index()) != 0 && voxel.z == 0)
}

fn solid_layer(
    kind: &ChunkStorage<voxel::Kind>,
    y: isize,
) -> [bool; Chunk::X_AXIS_SIZE * Chunk::Z_AXIS_SIZE] {
    let mut solid = [false; Chunk::X_AXIS_SIZE * Chunk::Z_AXIS_SIZE];

    if y < 0 || y >= Chunk::Y_AXIS_SIZE as isize {
        return solid;
    }

    for x in 0..Chunk::X_AXIS_SIZE {
        for z in 0..Chunk::Z_AXIS_SIZE {
            solid[x * Chunk::Z_AXIS_SIZE + z] = !kind
                .get(ChunkVoxel::new(x as u8, y as u8, z as u8))
                .is_none();
        }
    }

    solid
}

fn exposed_in_layer(
    x: usize,
    z: usize,
    solid: &[bool; Chunk::X_AXIS_SIZE * Chunk::Z_AXIS_SIZE],
    above: &[bool; Chunk::X_AXIS_SIZE * Chunk::Z_AXIS_SIZE],
    below: &[bool; Chunk::X_AXIS_SIZE * Chunk::Z_AXIS_SIZE],
) -> bool {
    if x == 0 || x + 1 == Chunk::X_AXIS_SIZE || z == 0 || z + 1 == Chunk::Z_AXIS_SIZE {
        return true;
    }

    let index = x * Chunk::Z_AXIS_SIZE + z;

    !solid[index - Chunk::Z_AXIS_SIZE]
        || !solid[index + Chunk::Z_AXIS_SIZE]
        || !solid[index - 1]
        || !solid[index + 1]
        || !above[index]
        || !below[index]
}

/// Like [`faces_occlusion`], but culls faces against a coarse neighbour using the same
/// conservative 3D cell rule as the coarse mesher. This keeps the full/coarse seam closed.
pub fn faces_occlusion_with_lod(
    kind: &ChunkStorage<voxel::Kind>,
    faces_occlusion: &mut ChunkStorage<voxel::FacesOcclusion>,
    neighboorhood: &[Option<&ChunkStorage<voxel::Kind>>; chunk::SIDE_COUNT],
    neighbor_lods: &[Option<u8>; chunk::SIDE_COUNT],
) {
    faces_occlusion_with_lod_skipping(
        kind,
        faces_occlusion,
        neighboorhood,
        neighbor_lods,
        &[LayerCoverage::Full; Chunk::Y_AXIS_SIZE],
    );
}

/// [`faces_occlusion_with_lod`] variant that skips sections already known to be fully occluded.
pub fn faces_occlusion_with_lod_skipping(
    kind: &ChunkStorage<voxel::Kind>,
    faces_occlusion: &mut ChunkStorage<voxel::FacesOcclusion>,
    neighboorhood: &[Option<&ChunkStorage<voxel::Kind>>; chunk::SIDE_COUNT],
    neighbor_lods: &[Option<u8>; chunk::SIDE_COUNT],
    coverage: &[LayerCoverage; Chunk::Y_AXIS_SIZE],
) {
    const DIM: IVec3 = IVec3::new(
        Chunk::X_AXIS_SIZE as i32,
        Chunk::Y_AXIS_SIZE as i32,
        Chunk::Z_AXIS_SIZE as i32,
    );

    faces_occlusion.fill(voxel::FacesOcclusion::fully_occluded());

    for (y, layer) in coverage.iter().copied().enumerate() {
        let boundary = match layer {
            LayerCoverage::Skip => continue,
            LayerCoverage::Boundary(boundary) => Some(boundary),
            LayerCoverage::Full => None,
        };

        let solid = if boundary.is_none() {
            Some((
                solid_layer(kind, y as isize),
                solid_layer(kind, y as isize + 1),
                solid_layer(kind, y as isize - 1),
            ))
        } else {
            None
        };

        for voxel in chunk::voxels_in_layer(y) {
            if let Some(boundary) = boundary
                && !is_boundary_voxel(voxel, boundary)
            {
                continue;
            }

            if let Some((solid, above, below)) = &solid
                && !exposed_in_layer(voxel.x as usize, voxel.z as usize, solid, above, below)
            {
                continue;
            }

            if kind.get(voxel).is_none() {
                continue;
            }

            let faces = voxel::SIDES
                .iter()
                .fold(FacesOcclusion::default(), |mut faces, &side| {
                    let neighbor = IVec3::from(voxel) + side.dir();

                    let occluded = if let Some(neighbor) = ChunkVoxel::try_from(neighbor) {
                        !kind.get(neighbor).is_none()
                    } else if let Some(chunk_side) = ChunkSide::from_voxel_side(side)
                        && let Some(neighbor_kind) = neighboorhood[chunk_side as usize]
                    {
                        let neighbor_chunk_voxel = math::euclid_rem(neighbor, DIM);
                        let neighbor_chunk_voxel = ChunkVoxel::try_from(neighbor_chunk_voxel)
                            .expect("Should be a valid voxel");

                        match neighbor_lods[chunk_side as usize] {
                            Some(lod) if lod > 0 => {
                                let scale = lod::LodScale::for_lod(lod);
                                let base = IVec3::new(
                                    (neighbor_chunk_voxel.x as i32 / scale.x) * scale.x,
                                    (neighbor_chunk_voxel.y as i32 / scale.y) * scale.y,
                                    (neighbor_chunk_voxel.z as i32 / scale.z) * scale.z,
                                );

                                lod::block_solid(neighbor_kind, base.x, base.y, base.z, scale)
                            }
                            _ => !neighbor_kind.get(neighbor_chunk_voxel).is_none(),
                        }
                    } else {
                        return faces;
                    };

                    faces.set(side, occluded);
                    faces
                });

            faces_occlusion.set(voxel, faces);
        }
    }
}

pub fn generate_faces(
    kind: &ChunkStorage<voxel::Kind>,
    occlusion: &ChunkStorage<voxel::FacesOcclusion>,
    soft_light: &ChunkStorage<voxel::FacesSoftLight>,
) -> Vec<voxel::Face> {
    generate_faces_skipping(
        kind,
        occlusion,
        soft_light,
        &[LayerCoverage::Full; Chunk::Y_AXIS_SIZE],
    )
}

/// [`generate_faces`] variant that skips sections known to contain no exposed faces.
pub fn generate_faces_skipping(
    kind: &ChunkStorage<voxel::Kind>,
    occlusion: &ChunkStorage<voxel::FacesOcclusion>,
    soft_light: &ChunkStorage<voxel::FacesSoftLight>,
    coverage: &[LayerCoverage; Chunk::Y_AXIS_SIZE],
) -> Vec<voxel::Face> {
    const FACES_ESTIMATION: usize = (Chunk::BUFFER_SIZE * voxel::SIDE_COUNT) / 2;

    let mut faces_vertices = Vec::with_capacity(FACES_ESTIMATION);

    for (y, layer) in coverage.iter().copied().enumerate() {
        let boundary = match layer {
            LayerCoverage::Skip => continue,
            LayerCoverage::Boundary(boundary) => Some(boundary),
            LayerCoverage::Full => None,
        };

        for voxel in chunk::voxels_in_layer(y) {
            if let Some(boundary) = boundary
                && !is_boundary_voxel(voxel, boundary)
            {
                continue;
            }

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
    }

    faces_vertices
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::light;

    #[test]
    fn kind_color_tints_vertex_light() {
        let kind = voxel::Kind::from_name("Sand").expect("Sand kind is configured");
        let face = voxel::Face {
            vertices: [ChunkVoxel::new(0, 0, 0); 4],
            side: voxel::Side::Up,
            kind,
            light: [15.0; 4],
        };

        let tint = voxel::KindsDescs::get().get_face_desc(&face);
        let expected = Vec3::new(tint.color.0, tint.color.1, tint.color.2);

        let vertices = generate_vertices(&[face]);

        assert!(!vertices.is_empty());
        for vertex in vertices {
            assert!(
                (vertex.light - expected).length() < 1e-5,
                "kind tint was not applied: {}",
                vertex.light
            );
        }
    }

    #[test]
    fn full_face_is_visible_against_partially_air_coarse_neighbor() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        kind.set(ChunkVoxel::new(15, 10, 0), voxel::Kind::id(1));

        let mut neighbor = ChunkStorage::<voxel::Kind>::default();
        for x in 0..=1 {
            for z in 0..=1 {
                neighbor.set(ChunkVoxel::new(x, 10, z), voxel::Kind::id(1));
            }
        }
        neighbor.set(ChunkVoxel::new(1, 10, 1), voxel::Kind::none());

        let mut occlusion = ChunkStorage::<voxel::FacesOcclusion>::default();
        let neighborhood = [Some(&neighbor), None, None, None];

        faces_occlusion_with_lod(
            &kind,
            &mut occlusion,
            &neighborhood,
            &[Some(1), None, None, None],
        );

        assert!(
            !occlusion
                .get(ChunkVoxel::new(15, 10, 0))
                .is_occluded(voxel::Side::Right),
            "a partially air coarse neighbour must keep the full-detail seam face visible"
        );

        for x in 0..=1 {
            for z in 0..=1 {
                neighbor.set(ChunkVoxel::new(x, 10, z), voxel::Kind::id(1));
            }
        }

        let neighborhood = [Some(&neighbor), None, None, None];

        faces_occlusion_with_lod(
            &kind,
            &mut occlusion,
            &neighborhood,
            &[Some(1), None, None, None],
        );

        assert!(
            occlusion
                .get(ChunkVoxel::new(15, 10, 0))
                .is_occluded(voxel::Side::Right),
            "a fully solid coarse neighbour must cull the seam face"
        );
    }

    #[test]
    fn layer_coverage_matches_full_meshing() {
        let chunk = Chunk::new(0, 0);
        let neighbor_chunk = Chunk::new(1, 0);
        let mut kind = ChunkStorage::<voxel::Kind>::default();

        for x in 0..Chunk::X_AXIS_SIZE {
            for z in 0..Chunk::Z_AXIS_SIZE {
                for y in 0..80 {
                    kind.set(
                        ChunkVoxel::new(x as u8, y as u8, z as u8),
                        voxel::Kind::id(1),
                    );
                }
            }
        }
        kind.set(ChunkVoxel::new(4, 40, 4), voxel::Kind::none());
        kind.pack();

        let mut light = ChunkStorage::<voxel::Light>::default();
        for x in 0..Chunk::X_AXIS_SIZE {
            for z in 0..Chunk::Z_AXIS_SIZE {
                light.set(
                    ChunkVoxel::new(x as u8, 80, z as u8),
                    voxel::Light::natural(voxel::Light::MAX_NATURAL_INTENSITY),
                );
            }
        }
        light.pack();

        let neighborhood = [Some(&kind); chunk::SIDE_COUNT];
        let neighbor_lods = [Some(0); chunk::SIDE_COUNT];
        let skipped = layer_coverage(&kind, &neighborhood);

        let mut full_occlusion = ChunkStorage::<voxel::FacesOcclusion>::default();
        faces_occlusion_with_lod(&kind, &mut full_occlusion, &neighborhood, &neighbor_lods);

        let mut skipped_occlusion = ChunkStorage::<voxel::FacesOcclusion>::default();
        faces_occlusion_with_lod_skipping(
            &kind,
            &mut skipped_occlusion,
            &neighborhood,
            &neighbor_lods,
            &skipped,
        );

        assert_eq!(full_occlusion, skipped_occlusion);

        let get_kind =
            |target: Chunk| (target == chunk || target == neighbor_chunk).then_some(&kind);
        let get_light =
            |target: Chunk| (target == chunk || target == neighbor_chunk).then_some(&light);

        let mut full_light = ChunkStorage::<voxel::FacesSoftLight>::default();
        light::smooth_lighting(chunk, &full_occlusion, &mut full_light, get_kind, get_light);

        let mut skipped_light = ChunkStorage::<voxel::FacesSoftLight>::default();
        light::smooth_lighting_skipping(
            chunk,
            &skipped_occlusion,
            &mut skipped_light,
            get_kind,
            get_light,
            &skipped,
        );

        assert_eq!(full_light, skipped_light);

        let full_faces = generate_faces(&kind, &full_occlusion, &full_light);
        let skipped_faces =
            generate_faces_skipping(&kind, &skipped_occlusion, &skipped_light, &skipped);

        assert_eq!(full_faces, skipped_faces);
    }

    #[test]
    fn boundary_coverage_matches_full_meshing_for_missing_neighbors() {
        let chunk = Chunk::new(0, 0);
        let mut kind = ChunkStorage::<voxel::Kind>::default();

        for x in 0..Chunk::X_AXIS_SIZE {
            for z in 0..Chunk::Z_AXIS_SIZE {
                for y in 0..80 {
                    kind.set(
                        ChunkVoxel::new(x as u8, y as u8, z as u8),
                        voxel::Kind::id(1),
                    );
                }
            }
        }
        kind.pack();

        let mut light = ChunkStorage::<voxel::Light>::default();
        for x in 0..Chunk::X_AXIS_SIZE {
            for z in 0..Chunk::Z_AXIS_SIZE {
                light.set(
                    ChunkVoxel::new(x as u8, 80, z as u8),
                    voxel::Light::natural(voxel::Light::MAX_NATURAL_INTENSITY),
                );
            }
        }
        light.pack();

        let neighborhood = [None; chunk::SIDE_COUNT];
        let coverage = layer_coverage(&kind, &neighborhood);

        let mut full_occlusion = ChunkStorage::<voxel::FacesOcclusion>::default();
        faces_occlusion_with_lod(
            &kind,
            &mut full_occlusion,
            &neighborhood,
            &[None; chunk::SIDE_COUNT],
        );

        let mut coverage_occlusion = ChunkStorage::<voxel::FacesOcclusion>::default();
        faces_occlusion_with_lod_skipping(
            &kind,
            &mut coverage_occlusion,
            &neighborhood,
            &[None; chunk::SIDE_COUNT],
            &coverage,
        );

        assert_eq!(full_occlusion, coverage_occlusion);

        let get_kind = |target: Chunk| (target == chunk).then_some(&kind);
        let get_light = |target: Chunk| (target == chunk).then_some(&light);

        let mut full_light = ChunkStorage::<voxel::FacesSoftLight>::default();
        light::smooth_lighting(chunk, &full_occlusion, &mut full_light, get_kind, get_light);

        let mut coverage_light = ChunkStorage::<voxel::FacesSoftLight>::default();
        light::smooth_lighting_skipping(
            chunk,
            &coverage_occlusion,
            &mut coverage_light,
            get_kind,
            get_light,
            &coverage,
        );

        assert_eq!(full_light, coverage_light);

        let full_faces = generate_faces(&kind, &full_occlusion, &full_light);
        let coverage_faces =
            generate_faces_skipping(&kind, &coverage_occlusion, &coverage_light, &coverage);

        assert_eq!(full_faces, coverage_faces);
    }

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

        kind.set((0, 0, 0).into(), 1.into());

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        let occ = faces_occlusion.get((0, 0, 0).into());

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

        kind.set((0, 0, 0).into(), 1.into());
        neighbor_kind.set((Chunk::X_END, 0, 0).into(), 1.into());
        neighborhood[voxel::Side::Left as usize] = Some(&neighbor_kind);

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        let occ = faces_occlusion.get((0, 0, 0).into());

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
