use bevy::math::{IVec3, Vec3};
use projekto_core::{
    chunk::{self, ChunkStorage},
    math,
    voxel::{self, FacesOcclusion},
};

pub(super) fn faces_occlusion(
    kind: &ChunkStorage<voxel::Kind>,
    faces_occlusion: &mut ChunkStorage<voxel::FacesOcclusion>,
    neighboorhood: &[Option<&ChunkStorage<voxel::Kind>>; voxel::SIDE_COUNT],
) {
    chunk::voxels().for_each(|voxel| {
        if kind.get(voxel).is_none() {
            faces_occlusion.set(voxel, voxel::FacesOcclusion::fully_occluded())
        } else {
            let mut faces = FacesOcclusion::default();
            voxel::SIDES.iter().for_each(|&side| {
                let neighbor = voxel + side.dir();

                let neighbor_kind = if chunk::is_within_bounds(neighbor) {
                    kind.get(neighbor)
                } else {
                    let Some(neighbor_kind) = neighboorhood[side as usize] else {
                        return;
                    };
                    let neighbor_chunk_voxel = math::euclid_rem(
                        neighbor,
                        IVec3::new(
                            chunk::X_AXIS_SIZE as i32,
                            chunk::Y_AXIS_SIZE as i32,
                            chunk::Z_AXIS_SIZE as i32,
                        ),
                    );
                    neighbor_kind.get(neighbor_chunk_voxel)
                };

                faces.set(side, !neighbor_kind.is_none());
            });
            faces_occlusion.set(voxel, faces);
        }
    })
}

enum AxisRange {
    Iter(std::ops::RangeInclusive<i32>),
    Rev(std::iter::Rev<std::ops::RangeInclusive<i32>>),
}

impl AxisRange {
    fn new(begin: i32, end_inclusive: i32) -> Self {
        if end_inclusive >= begin {
            Self::Iter(begin..=end_inclusive)
        } else {
            Self::Rev((begin..=end_inclusive).rev())
        }
    }
}

impl Iterator for AxisRange {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            AxisRange::Iter(it) => it.next(),
            AxisRange::Rev(it) => it.next(),
        }
    }
}

/// This function returns a [`Box`] dyn iterator since it can return either [`Range`] or
/// [`Rev<Iterator>`]
///
/// Returns** a boxed iterator to iterate over a given axis.
#[inline]
fn get_axis_range(axis: IVec3) -> AxisRange {
    let (begin, end) = match axis {
        IVec3::X => (0, chunk::X_END),
        IVec3::NEG_X => (chunk::X_END, 0),
        IVec3::Y => (0, chunk::Y_END),
        IVec3::NEG_Y => (chunk::Y_END, 0),
        IVec3::Z => (0, chunk::Z_END),
        IVec3::NEG_Z => (chunk::Z_END, 0),
        _ => unreachable!(),
    };
    AxisRange::new(begin, end)
}

/// Converts a swizzled Vector in it's conventional (X, Y, Z) format
///
/// Returns** a [`IVec3`] with X, Y and Z elements in order.
#[inline]
fn unswizzle(axis: (IVec3, IVec3, IVec3), a: i32, b: i32, c: i32) -> IVec3 {
    axis.0.abs() * a + axis.1.abs() * b + axis.2.abs() * c
}

/// The first tuple item is the outer most loop and the third item is the inner most.
///
/// Returns** a tuple indicating which direction the algorithm will walk in order to merge faces.
#[inline]
const fn get_side_walk_axis(side: voxel::Side) -> (IVec3, IVec3, IVec3) {
    match side {
        voxel::Side::Right => (IVec3::X, IVec3::Y, IVec3::NEG_Z),
        voxel::Side::Left => (IVec3::X, IVec3::Y, IVec3::Z),
        voxel::Side::Up => (IVec3::Y, IVec3::NEG_Z, IVec3::X),
        voxel::Side::Down => (IVec3::Y, IVec3::Z, IVec3::X),
        voxel::Side::Front => (IVec3::Z, IVec3::Y, IVec3::X),
        voxel::Side::Back => (IVec3::Z, IVec3::Y, IVec3::NEG_X),
    }
}

struct MergerIterator {
    walk_axis: (IVec3, IVec3, IVec3),
    a_range: AxisRange,
    b_range: AxisRange,
    c_range: AxisRange,

    a: i32,
    b: i32,
}

impl MergerIterator {
    fn new(side: voxel::Side) -> MergerIterator {
        let walk_axis = get_side_walk_axis(side);
        let a_range = get_axis_range(walk_axis.0);
        let b_range = get_axis_range(walk_axis.1);
        let c_range = get_axis_range(walk_axis.2);

        MergerIterator {
            walk_axis,
            a_range,
            b_range,
            c_range,
            a: -1,
            b: -1,
        }
    }
}

impl Iterator for MergerIterator {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        // When a is -1, next range value
        if self.a == -1 {
            self.a = self.a_range.next()?;
        }

        // When b is -1, invalidate a and reset b range
        if self.b == -1 {
            if let Some(b) = self.b_range.next() {
                self.b = b;
            } else {
                self.a = -1;
                self.b_range = get_axis_range(self.walk_axis.1);
                return self.next();
            }
        }

        if let Some(c) = self.c_range.next() {
            Some(unswizzle(self.walk_axis, self.a, self.b, c))
        } else {
            self.b = -1;
            self.c_range = get_axis_range(self.walk_axis.2);

            self.next()
        }
    }
}

#[inline]
fn should_skip_voxel(
    merged: &[bool],
    voxel: IVec3,
    side: voxel::Side,
    kind: voxel::Kind,
    chunk: &ChunkData,
) -> bool {
    kind.is_none()
        || merged[chunk::to_index(voxel)]
        || chunk.faces_occlusion.get(voxel).is_occluded(side)
}

#[inline]
fn should_merge(
    voxel: IVec3,
    next_voxel: IVec3,
    merged: &[bool],
    side: voxel::Side,
    chunk: &ChunkData,
) -> bool {
    chunk::is_within_bounds(next_voxel)
        && !should_skip_voxel(merged, next_voxel, side, chunk.kind.get(next_voxel), chunk)
        && chunk.kind.get(voxel) == chunk.kind.get(next_voxel)
        && chunk.faces_soft_light.get(voxel).get(side)
            == chunk.faces_soft_light.get(next_voxel).get(side)
}

/// Finds the furthest equal voxel from the given begin point, into the step direction.
#[inline]
fn find_furthest_eq_voxel(
    begin: IVec3,
    step: IVec3,
    merged: &[bool],
    side: voxel::Side,
    until: Option<IVec3>,
    chunk: &ChunkData,
) -> IVec3 {
    let mut next_voxel = begin + step;

    while should_merge(begin, next_voxel, merged, side, chunk) {
        if let Some(target) = until {
            if target == next_voxel {
                return next_voxel;
            }
        }

        next_voxel += step;
    }

    next_voxel -= step;

    next_voxel
}

/// Generates a list of voxels, based on v1, v2 and v3 inclusive, which was walked.
#[inline]
fn calc_walked_voxels(
    v1: IVec3,
    v2: IVec3,
    v3: IVec3,
    perpendicular_axis: IVec3,
    current_axis: IVec3,
) -> Vec<IVec3> {
    let mut walked_voxels = vec![];

    let mut begin = v1;
    let mut current = begin;
    let mut end = v2;

    while current != v3 {
        walked_voxels.push(current);
        if current == end {
            begin += perpendicular_axis;
            end += perpendicular_axis;
            current = begin;
        } else {
            current += current_axis;
        }
    }

    walked_voxels.push(current);

    walked_voxels
}

struct ChunkData<'a> {
    kind: &'a ChunkStorage<voxel::Kind>,
    faces_occlusion: &'a ChunkStorage<voxel::FacesOcclusion>,
    faces_soft_light: &'a ChunkStorage<voxel::FacesSoftLight>,
}

pub(super) fn faces_merge(
    kind: &ChunkStorage<voxel::Kind>,
    faces_occlusion: &ChunkStorage<voxel::FacesOcclusion>,
    faces_soft_light: &ChunkStorage<voxel::FacesSoftLight>,
) -> Vec<voxel::Face> {
    let mut faces_vertices = vec![];

    let chunk = ChunkData {
        kind,
        faces_occlusion,
        faces_soft_light,
    };

    for side in voxel::SIDES {
        let walk_axis = get_side_walk_axis(side);
        let mut merged = vec![false; chunk::BUFFER_SIZE];

        for voxel in MergerIterator::new(side) {
            // Due to cache friendliness, the current axis is always the deepest on nested loop
            let current_axis = walk_axis.2;
            let perpendicular_axis = walk_axis.1;

            let kind = chunk.kind.get(voxel);

            if should_skip_voxel(&merged, voxel, side, kind, &chunk) {
                continue;
            }

            let smooth_light = chunk.faces_soft_light.get(voxel);

            // Finds the furthest equal voxel on current axis
            let v1 = voxel;
            let v2 = find_furthest_eq_voxel(voxel, current_axis, &merged, side, None, &chunk);

            // Finds the furthest equal voxel on perpendicular axis
            let perpendicular_step = perpendicular_axis;
            let mut v3 = v2 + perpendicular_step;

            // The loop walks all the way up on current_axis and than stepping one unit at time on
            // perpendicular_axis. This walk it'll be possible to find the next vertex
            // (v3) which is be able to merge with v1 and v2
            let mut next_begin_voxel = v1 + perpendicular_step;
            while should_merge(voxel, next_begin_voxel, &merged, side, &chunk) {
                let furthest = find_furthest_eq_voxel(
                    next_begin_voxel,
                    current_axis,
                    &merged,
                    side,
                    Some(v3),
                    &chunk,
                );

                if furthest == v3 {
                    v3 += perpendicular_step;
                    next_begin_voxel += perpendicular_step;
                } else {
                    break;
                }
            }

            // At this point, v3 is out-of-bounds or points to a voxel which can't be merged, so
            // step-back one unit
            v3 -= perpendicular_step;

            // Flag walked voxels, making a perfect square from v1, v2 and v3, on the given axis.
            for voxel in calc_walked_voxels(v1, v2, v3, perpendicular_axis, current_axis) {
                merged[chunk::to_index(voxel)] = true;
            }

            // v4 can be inferred com v1, v2 and v3
            let v4 = v1 + (v3 - v2);

            faces_vertices.push(voxel::Face {
                vertices: [v1, v2, v3, v4],
                side,
                kind,
                light: smooth_light.get(side),
            })
        }
    }

    faces_vertices
}

// v3               v2
// +-----------+
// v7  / |      v6 / |
// +-----------+   |
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
pub(super) fn generate_vertices(faces: Vec<voxel::Face>) -> Vec<voxel::Vertex> {
    let mut vertices = vec![];
    let kinds_descs = voxel::KindsDescs::get();
    let tile_texture_size = (kinds_descs.count_tiles() as f32).recip();

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

        let light_fraction = (voxel::Light::MAX_NATURAL_INTENSITY as f32).recip();

        for (i, v) in faces_vertices.into_iter().enumerate() {
            vertices.push(voxel::Vertex {
                position: v,
                normal,
                uv: tile_uv[i],
                tile_coord_start,
                light: Vec3::splat(face.light[i] * light_fraction),
            });
        }
    }

    debug_assert!(!vertices.is_empty());
    vertices
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn faces_occlusion_empty_chunk() {
        let kind = Default::default();
        let mut faces_occlusion = Default::default();
        let neighborhood = [None; voxel::SIDE_COUNT];

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        assert!(
            faces_occlusion.is_fully_occluded(),
            "Should be fully occluded in an empty chunk"
        );
    }

    #[test]
    fn faces_occlusion_opaque_voxel() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut faces_occlusion = Default::default();
        let neighborhood = [None; voxel::SIDE_COUNT];

        kind.set([0, 0, 0].into(), 1.into());

        super::faces_occlusion(&kind, &mut faces_occlusion, &neighborhood);

        let occ = faces_occlusion.get([0, 0, 0].into());

        voxel::SIDES.iter().for_each(|&side| {
            assert!(!occ.is_occluded(side), "No side should be occluded");
        })
    }

    #[test]
    fn faces_occlusion_neighbor() {
        let mut kind = ChunkStorage::<voxel::Kind>::default();
        let mut neighbor_kind = ChunkStorage::<voxel::Kind>::default();
        let mut faces_occlusion = Default::default();
        let mut neighborhood = [None; voxel::SIDE_COUNT];

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
        })
    }
}
