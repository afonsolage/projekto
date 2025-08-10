#![allow(unused)]
use bevy::math::IVec3;

use projekto_core::{
    chunk::{self, ChunkStorage},
    voxel::{self},
};

enum AxisRange {
    Iter(std::ops::RangeInclusive<i32>),
    Rev(std::iter::Rev<std::ops::RangeInclusive<i32>>),
}

impl AxisRange {
    fn new(begin: i32, end_inclusive: i32) -> Self {
        if end_inclusive >= begin {
            Self::Iter(begin..=end_inclusive)
        } else {
            Self::Rev((end_inclusive..=begin).rev())
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
/// **Returns** a boxed iterator to iterate over a given axis.
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
/// **Returns** a [`IVec3`] with X, Y and Z elements in order.
#[inline]
fn unswizzle(axis: (IVec3, IVec3, IVec3), a: i32, b: i32, c: i32) -> IVec3 {
    axis.0.abs() * a + axis.1.abs() * b + axis.2.abs() * c
}

/// The first tuple item is the outer most loop and the third item is the inner most.
///
/// **Returns** a tuple indicating which direction the algorithm will walk in order to merge
/// faces.
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
    chunk::is_inside(next_voxel)
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

pub fn generate_faces(
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

    let mut merged = vec![false; chunk::BUFFER_SIZE];

    for side in voxel::SIDES {
        let walk_axis = get_side_walk_axis(side);

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

            // The loop walks all the way up on current_axis and than stepping one unit at time
            // on perpendicular_axis. This walk it'll be possible to find the
            // next vertex (v3) which is be able to merge with v1 and v2
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

            // Flag walked voxels, making a perfect square from v1, v2 and v3, on the given
            // axis.
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn merge_right_faces() {
        // +-------------------+        +-------------------+
        // 4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
        // +-------------------+        +-------- 2 -   ----+
        // 3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
        // +-------------------+        +------------   ----+
        // Y     2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        // |        +-------------------+        +----       --------+
        // |     1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        // Z -----+        +-------------------+        +----       --------+
        // 0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
        // +-------------------+        +-------------------+
        //
        // + 4   3   2   1   0
        //
        // Merge direction (-Z, Y)[->, ^]

        let mut kinds = ChunkStorage::<voxel::Kind>::default();

        kinds.set((0, 0, 0).into(), 1.into());
        kinds.set((0, 0, 2).into(), 1.into());
        kinds.set((0, 0, 3).into(), 1.into());
        kinds.set((0, 1, 1).into(), 1.into());
        kinds.set((0, 1, 2).into(), 1.into());
        kinds.set((0, 1, 3).into(), 1.into());
        kinds.set((0, 2, 1).into(), 2.into());
        kinds.set((0, 2, 2).into(), 1.into());
        kinds.set((0, 2, 3).into(), 1.into());
        kinds.set((0, 3, 1).into(), 2.into());
        kinds.set((0, 3, 2).into(), 2.into());
        kinds.set((0, 4, 1).into(), 2.into());
        kinds.set((0, 4, 2).into(), 2.into());

        let merged = super::generate_faces(&kinds, &Default::default(), &Default::default())
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Right) // We care only for right faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<voxel::Face> = vec![
            voxel::Face {
                vertices: [
                    (0, 0, 3).into(),
                    (0, 0, 2).into(),
                    (0, 2, 2).into(),
                    (0, 2, 3).into(),
                ],
                side: voxel::Side::Right,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 0, 0).into(),
                    (0, 0, 0).into(),
                    (0, 0, 0).into(),
                    (0, 0, 0).into(),
                ],
                side: voxel::Side::Right,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 1, 1).into(),
                    (0, 1, 1).into(),
                    (0, 1, 1).into(),
                    (0, 1, 1).into(),
                ],
                side: voxel::Side::Right,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 2, 1).into(),
                    (0, 2, 1).into(),
                    (0, 4, 1).into(),
                    (0, 4, 1).into(),
                ],
                side: voxel::Side::Right,
                kind: 2.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 3, 2).into(),
                    (0, 3, 2).into(),
                    (0, 4, 2).into(),
                    (0, 4, 2).into(),
                ],
                side: voxel::Side::Right,
                kind: 2.into(),
                ..Default::default()
            },
        ];

        assert_eq!(&merged.len(), &test_merged.len());

        test_merged.into_iter().enumerate().for_each(|(i, f)| {
            assert_eq!(&merged[i], &f, "Failed on index {}", i);
        });
    }

    #[test]
    fn merge_left_faces() {
        // +-------------------+        +-------------------+
        // 4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
        // +-------------------+        +-------- 2 -   ----+
        // 3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
        // +-------------------+        +------------   ----+
        // Y            2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        // |               +-------------------+        +----       --------+
        // |            1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        // +----- Z        +-------------------+        +----       --------+
        // 0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
        // +-------------------+        +-------------------+
        //
        // + 0   1   2   3   4
        //
        // Merge direction (Z, Y)[->, ^]

        let mut kinds = ChunkStorage::<voxel::Kind>::default();

        kinds.set((0, 0, 1).into(), 1.into());
        kinds.set((0, 0, 2).into(), 1.into());
        kinds.set((0, 0, 4).into(), 1.into());
        kinds.set((0, 1, 1).into(), 1.into());
        kinds.set((0, 1, 2).into(), 1.into());
        kinds.set((0, 1, 3).into(), 1.into());
        kinds.set((0, 2, 1).into(), 1.into());
        kinds.set((0, 2, 2).into(), 1.into());
        kinds.set((0, 2, 3).into(), 2.into());
        kinds.set((0, 3, 2).into(), 2.into());
        kinds.set((0, 3, 3).into(), 2.into());
        kinds.set((0, 4, 2).into(), 2.into());
        kinds.set((0, 4, 3).into(), 2.into());

        let merged = super::generate_faces(&kinds, &Default::default(), &Default::default())
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Left) // We care only for left faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<voxel::Face> = vec![
            voxel::Face {
                vertices: [
                    (0, 0, 1).into(),
                    (0, 0, 2).into(),
                    (0, 2, 2).into(),
                    (0, 2, 1).into(),
                ],
                side: voxel::Side::Left,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 0, 4).into(),
                    (0, 0, 4).into(),
                    (0, 0, 4).into(),
                    (0, 0, 4).into(),
                ],
                side: voxel::Side::Left,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 1, 3).into(),
                    (0, 1, 3).into(),
                    (0, 1, 3).into(),
                    (0, 1, 3).into(),
                ],
                side: voxel::Side::Left,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 2, 3).into(),
                    (0, 2, 3).into(),
                    (0, 4, 3).into(),
                    (0, 4, 3).into(),
                ],
                side: voxel::Side::Left,
                kind: 2.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 3, 2).into(),
                    (0, 3, 2).into(),
                    (0, 4, 2).into(),
                    (0, 4, 2).into(),
                ],
                side: voxel::Side::Left,
                kind: 2.into(),
                ..Default::default()
            },
        ];

        assert_eq!(&merged.len(), &test_merged.len());

        test_merged.into_iter().enumerate().for_each(|(i, f)| {
            assert_eq!(&merged[i], &f, "Failed on index {}", i);
        });
    }

    #[test]
    fn merge_up_faces() {
        // +-------------------+        +-------------------+
        // 0  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
        // +-------------------+        +-------- 2 -   ----+
        // 1  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
        // +-------------------+        +------------   ----+
        // +----- X     2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        // |               +-------------------+        +----       --------+
        // |            3  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        // Z               +-------------------+        +----       --------+
        // 4  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
        // +-------------------+        +-------------------+
        //
        // + 0   1   2   3   4
        //
        // Merge direction (X, -Z)[->, ^]

        let mut kinds = ChunkStorage::<voxel::Kind>::default();

        kinds.set((1, 0, 4).into(), 1.into());
        kinds.set((2, 0, 4).into(), 1.into());
        kinds.set((4, 0, 4).into(), 1.into());
        kinds.set((1, 0, 3).into(), 1.into());
        kinds.set((2, 0, 3).into(), 1.into());
        kinds.set((3, 0, 3).into(), 1.into());
        kinds.set((1, 0, 2).into(), 1.into());
        kinds.set((2, 0, 2).into(), 1.into());
        kinds.set((3, 0, 2).into(), 2.into());
        kinds.set((2, 0, 1).into(), 2.into());
        kinds.set((3, 0, 1).into(), 2.into());
        kinds.set((2, 0, 0).into(), 2.into());
        kinds.set((3, 0, 0).into(), 2.into());

        let merged = super::generate_faces(&kinds, &Default::default(), &Default::default())
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Up) // We care only for Up faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<voxel::Face> = vec![
            voxel::Face {
                vertices: [
                    (1, 0, 4).into(),
                    (2, 0, 4).into(),
                    (2, 0, 2).into(),
                    (1, 0, 2).into(),
                ],
                side: voxel::Side::Up,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (4, 0, 4).into(),
                    (4, 0, 4).into(),
                    (4, 0, 4).into(),
                    (4, 0, 4).into(),
                ],
                side: voxel::Side::Up,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (3, 0, 3).into(),
                    (3, 0, 3).into(),
                    (3, 0, 3).into(),
                    (3, 0, 3).into(),
                ],
                side: voxel::Side::Up,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (3, 0, 2).into(),
                    (3, 0, 2).into(),
                    (3, 0, 0).into(),
                    (3, 0, 0).into(),
                ],
                side: voxel::Side::Up,
                kind: 2.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (2, 0, 1).into(),
                    (2, 0, 1).into(),
                    (2, 0, 0).into(),
                    (2, 0, 0).into(),
                ],
                side: voxel::Side::Up,
                kind: 2.into(),
                ..Default::default()
            },
        ];

        assert_eq!(&merged.len(), &test_merged.len());

        test_merged.into_iter().enumerate().for_each(|(i, f)| {
            assert_eq!(&merged[i], &f, "Failed on index {}", i);
        });
    }

    #[test]
    fn merge_up_faces_sub_divide() {
        // +-------------------+        +-------------------+
        // 0  | 0 | 1 | 1 | 0 | 0 |        | 0 |       | 0 | 0 |
        // +-------------------+        +----   1   --------+
        // 1  | 0 | 1 | 1 | 0 | 0 |        | 0 |       | 0 | 0 |
        // +-------------------+        +-------------------+
        // +----- X     2  | 0 | 1 | 2 | 0 | 0 |   ->   | 0 | 1 | 2 | 0 | 0 |
        // |               +-------------------+        +-------------------+
        // |            3  | 0 | 1 | 1 | 0 | 0 |        | 0 |       | 0 | 0 |
        // Z               +-------------------+        +----   1   --------+
        // 4  | 0 | 1 | 1 | 0 | 0 |        | 0 |       | 0 | 0 |
        // +-------------------+        +-------------------+
        //
        // + 0   1   2   3   4
        //
        // Merge direction (X, -Z)[->, ^]

        let mut kinds = ChunkStorage::<voxel::Kind>::default();

        kinds.set((1, 0, 4).into(), 1.into());
        kinds.set((2, 0, 4).into(), 1.into());
        kinds.set((1, 0, 3).into(), 1.into());
        kinds.set((2, 0, 3).into(), 1.into());
        kinds.set((1, 0, 2).into(), 1.into());
        kinds.set((2, 0, 2).into(), 2.into());
        kinds.set((1, 0, 1).into(), 1.into());
        kinds.set((2, 0, 1).into(), 1.into());
        kinds.set((1, 0, 0).into(), 1.into());
        kinds.set((2, 0, 0).into(), 1.into());

        let merged = super::generate_faces(&kinds, &Default::default(), &Default::default())
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Up) // We care only for Up faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<voxel::Face> = vec![
            voxel::Face {
                vertices: [
                    (1, 0, 4).into(),
                    (2, 0, 4).into(),
                    (2, 0, 3).into(),
                    (1, 0, 3).into(),
                ],
                side: voxel::Side::Up,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (1, 0, 2).into(),
                    (1, 0, 2).into(),
                    (1, 0, 2).into(),
                    (1, 0, 2).into(),
                ],
                side: voxel::Side::Up,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (2, 0, 2).into(),
                    (2, 0, 2).into(),
                    (2, 0, 2).into(),
                    (2, 0, 2).into(),
                ],
                side: voxel::Side::Up,
                kind: 2.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (1, 0, 1).into(),
                    (2, 0, 1).into(),
                    (2, 0, 0).into(),
                    (1, 0, 0).into(),
                ],
                side: voxel::Side::Up,
                kind: 1.into(),
                ..Default::default()
            },
        ];

        assert_eq!(&merged.len(), &test_merged.len());

        test_merged.into_iter().enumerate().for_each(|(i, f)| {
            assert_eq!(&merged[i], &f, "Failed on index {}", i);
        });
    }

    #[test]
    fn merge_down_faces() {
        // +-------------------+        +-------------------+
        // 4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
        // +-------------------+        +-------- 2 -   ----+
        // 3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
        // +-------------------+        +------------   ----+
        // Z            2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        // |               +-------------------+        +----       --------+
        // |            1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        // +----- X        +-------------------+        +----       --------+
        // 0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
        // +-------------------+        +-------------------+
        //
        // + 0   1   2   3   4
        //
        // Merge direction (X, Z)[->, ^]

        let mut kinds = ChunkStorage::<voxel::Kind>::default();

        kinds.set((1, 0, 0).into(), 1.into());
        kinds.set((2, 0, 0).into(), 1.into());
        kinds.set((4, 0, 0).into(), 1.into());
        kinds.set((1, 0, 1).into(), 1.into());
        kinds.set((2, 0, 1).into(), 1.into());
        kinds.set((3, 0, 1).into(), 1.into());
        kinds.set((1, 0, 2).into(), 1.into());
        kinds.set((2, 0, 2).into(), 1.into());
        kinds.set((3, 0, 2).into(), 2.into());
        kinds.set((2, 0, 3).into(), 2.into());
        kinds.set((3, 0, 3).into(), 2.into());
        kinds.set((2, 0, 4).into(), 2.into());
        kinds.set((3, 0, 4).into(), 2.into());

        let merged = super::generate_faces(&kinds, &Default::default(), &Default::default())
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Down) // We care only for Down faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<voxel::Face> = vec![
            voxel::Face {
                vertices: [
                    (1, 0, 0).into(),
                    (2, 0, 0).into(),
                    (2, 0, 2).into(),
                    (1, 0, 2).into(),
                ],
                side: voxel::Side::Down,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (4, 0, 0).into(),
                    (4, 0, 0).into(),
                    (4, 0, 0).into(),
                    (4, 0, 0).into(),
                ],
                side: voxel::Side::Down,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (3, 0, 1).into(),
                    (3, 0, 1).into(),
                    (3, 0, 1).into(),
                    (3, 0, 1).into(),
                ],
                side: voxel::Side::Down,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (3, 0, 2).into(),
                    (3, 0, 2).into(),
                    (3, 0, 4).into(),
                    (3, 0, 4).into(),
                ],
                side: voxel::Side::Down,
                kind: 2.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (2, 0, 3).into(),
                    (2, 0, 3).into(),
                    (2, 0, 4).into(),
                    (2, 0, 4).into(),
                ],
                side: voxel::Side::Down,
                kind: 2.into(),
                ..Default::default()
            },
        ];

        assert_eq!(&merged.len(), &test_merged.len());

        test_merged.into_iter().enumerate().for_each(|(i, f)| {
            assert_eq!(&merged[i], &f, "Failed on index {}", i);
        });
    }

    #[test]
    fn merge_front_faces() {
        // +-------------------+        +-------------------+
        // 4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
        // +-------------------+        +-------- 2 -   ----+
        // 3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
        // +-------------------+        +------------   ----+
        // Y            2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        // |               +-------------------+        +----       --------+
        // |            1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        // +----- X        +-------------------+        +----       --------+
        // 0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
        // +-------------------+        +-------------------+
        //
        // + 0   1   2   3   4
        //
        // Merge direction (X, Y)[->, ^]

        let mut kinds = ChunkStorage::<voxel::Kind>::default();

        kinds.set((1, 0, 0).into(), 1.into());
        kinds.set((2, 0, 0).into(), 1.into());
        kinds.set((4, 0, 0).into(), 1.into());
        kinds.set((1, 1, 0).into(), 1.into());
        kinds.set((2, 1, 0).into(), 1.into());
        kinds.set((3, 1, 0).into(), 1.into());
        kinds.set((1, 2, 0).into(), 1.into());
        kinds.set((2, 2, 0).into(), 1.into());
        kinds.set((3, 2, 0).into(), 2.into());
        kinds.set((2, 3, 0).into(), 2.into());
        kinds.set((3, 3, 0).into(), 2.into());
        kinds.set((2, 4, 0).into(), 2.into());
        kinds.set((3, 4, 0).into(), 2.into());

        let merged = super::generate_faces(&kinds, &Default::default(), &Default::default())
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Front) // We care only for Front faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<voxel::Face> = vec![
            voxel::Face {
                vertices: [
                    (1, 0, 0).into(),
                    (2, 0, 0).into(),
                    (2, 2, 0).into(),
                    (1, 2, 0).into(),
                ],
                side: voxel::Side::Front,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (4, 0, 0).into(),
                    (4, 0, 0).into(),
                    (4, 0, 0).into(),
                    (4, 0, 0).into(),
                ],
                side: voxel::Side::Front,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (3, 1, 0).into(),
                    (3, 1, 0).into(),
                    (3, 1, 0).into(),
                    (3, 1, 0).into(),
                ],
                side: voxel::Side::Front,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (3, 2, 0).into(),
                    (3, 2, 0).into(),
                    (3, 4, 0).into(),
                    (3, 4, 0).into(),
                ],
                side: voxel::Side::Front,
                kind: 2.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (2, 3, 0).into(),
                    (2, 3, 0).into(),
                    (2, 4, 0).into(),
                    (2, 4, 0).into(),
                ],
                side: voxel::Side::Front,
                kind: 2.into(),
                ..Default::default()
            },
        ];

        assert_eq!(&merged.len(), &test_merged.len());

        test_merged.into_iter().enumerate().for_each(|(i, f)| {
            assert_eq!(&merged[i], &f, "Failed on index {}", i);
        });
    }

    #[test]
    fn merge_back_faces() {
        // +-------------------+        +-------------------+
        // 4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
        // +-------------------+        +-------- 2 -   ----+
        // 3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
        // +-------------------+        +------------   ----+
        // Y     2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        // |        +-------------------+        +----       --------+
        // |     1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        // X -----+        +-------------------+        +----       --------+
        // 0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
        // +-------------------+        +-------------------+
        //
        // + 4   3   2   1   0
        //
        // Merge direction (-X, Y)[->, ^]

        let mut kinds = ChunkStorage::<voxel::Kind>::default();

        kinds.set((3, 0, 0).into(), 1.into());
        kinds.set((2, 0, 0).into(), 1.into());
        kinds.set((0, 0, 0).into(), 1.into());
        kinds.set((3, 1, 0).into(), 1.into());
        kinds.set((2, 1, 0).into(), 1.into());
        kinds.set((1, 1, 0).into(), 1.into());
        kinds.set((3, 2, 0).into(), 1.into());
        kinds.set((2, 2, 0).into(), 1.into());
        kinds.set((1, 2, 0).into(), 2.into());
        kinds.set((2, 3, 0).into(), 2.into());
        kinds.set((1, 3, 0).into(), 2.into());
        kinds.set((2, 4, 0).into(), 2.into());
        kinds.set((1, 4, 0).into(), 2.into());

        let merged = super::generate_faces(&kinds, &Default::default(), &Default::default())
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Back) // We care only for Back faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<voxel::Face> = vec![
            voxel::Face {
                vertices: [
                    (3, 0, 0).into(),
                    (2, 0, 0).into(),
                    (2, 2, 0).into(),
                    (3, 2, 0).into(),
                ],
                side: voxel::Side::Back,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (0, 0, 0).into(),
                    (0, 0, 0).into(),
                    (0, 0, 0).into(),
                    (0, 0, 0).into(),
                ],
                side: voxel::Side::Back,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (1, 1, 0).into(),
                    (1, 1, 0).into(),
                    (1, 1, 0).into(),
                    (1, 1, 0).into(),
                ],
                side: voxel::Side::Back,
                kind: 1.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (1, 2, 0).into(),
                    (1, 2, 0).into(),
                    (1, 4, 0).into(),
                    (1, 4, 0).into(),
                ],
                side: voxel::Side::Back,
                kind: 2.into(),
                ..Default::default()
            },
            voxel::Face {
                vertices: [
                    (2, 3, 0).into(),
                    (2, 3, 0).into(),
                    (2, 4, 0).into(),
                    (2, 4, 0).into(),
                ],
                side: voxel::Side::Back,
                kind: 2.into(),
                ..Default::default()
            },
        ];

        assert_eq!(&merged.len(), &test_merged.len());

        test_merged.into_iter().enumerate().for_each(|(i, f)| {
            assert_eq!(&merged[i], &f, "Failed on index {}", i);
        });
    }

    #[test]
    fn calc_walked_voxels_negative_axis() {
        let v1 = (0, 0, 3).into();
        let v2 = (0, 0, 2).into();
        let v3 = (0, 2, 2).into();
        let current_axis = (0, 0, -1).into();
        let perpendicular_axis = (0, 1, 0).into();

        let walked = super::calc_walked_voxels(v1, v2, v3, perpendicular_axis, current_axis);
        let test_walked: Vec<IVec3> = vec![
            (0, 0, 3).into(),
            (0, 0, 2).into(),
            (0, 1, 3).into(),
            (0, 1, 2).into(),
            (0, 2, 3).into(),
            (0, 2, 2).into(),
        ];

        assert_eq!(&walked.len(), &test_walked.len());

        test_walked.into_iter().enumerate().for_each(|(i, w)| {
            assert_eq!(walked[i], w, "Failed on index {}", i);
        });
    }

    #[test]
    fn calc_walked_voxels_negative_x_axis() {
        let v1 = (3, 0, 0).into();
        let v2 = (2, 0, 0).into();
        let v3 = (2, 2, 0).into();
        let current_axis = (-1, 0, 0).into();
        let perpendicular_axis = (0, 1, 0).into();

        let walked = super::calc_walked_voxels(v1, v2, v3, perpendicular_axis, current_axis);
        let test_walked: Vec<IVec3> = vec![
            (3, 0, 0).into(),
            (2, 0, 0).into(),
            (3, 1, 0).into(),
            (2, 1, 0).into(),
            (3, 2, 0).into(),
            (2, 2, 0).into(),
        ];

        assert_eq!(&walked.len(), &test_walked.len());

        test_walked.into_iter().enumerate().for_each(|(i, w)| {
            assert_eq!(walked[i], w, "Failed on index {}", i);
        });
    }

    #[test]
    fn calc_walked_voxels_positive_axis() {
        let v1 = (1, 2, 0).into();
        let v2 = (4, 2, 0).into();
        let v3 = (4, 4, 0).into();
        let current_axis = (1, 0, 0).into();
        let perpendicular_axis = (0, 1, 0).into();

        let walked = super::calc_walked_voxels(v1, v2, v3, perpendicular_axis, current_axis);
        let test_walked: Vec<IVec3> = vec![
            (1, 2, 0).into(),
            (2, 2, 0).into(),
            (3, 2, 0).into(),
            (4, 2, 0).into(),
            (1, 3, 0).into(),
            (2, 3, 0).into(),
            (3, 3, 0).into(),
            (4, 3, 0).into(),
            (1, 4, 0).into(),
            (2, 4, 0).into(),
            (3, 4, 0).into(),
            (4, 4, 0).into(),
        ];

        assert_eq!(&walked.len(), &test_walked.len());

        test_walked.into_iter().enumerate().for_each(|(i, w)| {
            assert_eq!(walked[i], w, "Failed on index {}", i);
        });
    }

    #[test]
    fn unswizzle() {
        let walk_axis = ((0, 1, 0).into(), (0, 0, 1).into(), (-1, 0, 0).into());

        let unswizzled = super::unswizzle(walk_axis, 3, 5, 2);

        assert_eq!(unswizzled, (2, 3, 5).into());
    }

    #[test]
    fn merger_iterator_right() {
        let side = voxel::Side::Right;

        let merger_it = MergerIterator::new(side).collect::<Vec<_>>();
        let normal_it = {
            let mut vec = vec![];
            for x in 0..chunk::X_AXIS_SIZE {
                for y in 0..chunk::Y_AXIS_SIZE {
                    for z in (0..=chunk::Z_END).rev() {
                        vec.push(IVec3::new(x as i32, y as i32, z));
                    }
                }
            }
            vec
        };

        assert!(!merger_it.is_empty(), "Merger iterator must be non empty");

        merger_it.into_iter().enumerate().for_each(|(i, v)| {
            assert_eq!(&normal_it[i], &v);
        })
    }

    #[test]
    fn merger_iterator_left() {
        let side = voxel::Side::Left;

        let merger_it = MergerIterator::new(side).collect::<Vec<_>>();
        let normal_it = {
            let mut vec = vec![];
            for x in 0..chunk::X_AXIS_SIZE {
                for y in 0..chunk::Y_AXIS_SIZE {
                    for z in 0..chunk::Z_AXIS_SIZE {
                        vec.push(IVec3::new(x as i32, y as i32, z as i32));
                    }
                }
            }
            vec
        };

        assert!(!merger_it.is_empty(), "Merger iterator must be non empty");

        merger_it.into_iter().enumerate().for_each(|(i, v)| {
            assert_eq!(&normal_it[i], &v);
        })
    }

    #[test]
    fn merger_iterator_up() {
        let side = voxel::Side::Up;

        let merger_it = MergerIterator::new(side).collect::<Vec<_>>();
        let normal_it = {
            let mut vec = vec![];
            for y in 0..chunk::Y_AXIS_SIZE {
                for z in (0..=chunk::Z_END).rev() {
                    for x in 0..chunk::X_AXIS_SIZE {
                        vec.push(IVec3::new(x as i32, y as i32, z));
                    }
                }
            }
            vec
        };

        assert!(!merger_it.is_empty(), "Merger iterator must be non empty");

        merger_it.into_iter().enumerate().for_each(|(i, v)| {
            assert_eq!(&normal_it[i], &v, "Failed to match at index {}", i);
        })
    }

    #[test]
    fn merger_iterator_down() {
        let side = voxel::Side::Down;

        let merger_it = MergerIterator::new(side).collect::<Vec<_>>();
        let normal_it = {
            let mut vec = vec![];
            for y in 0..chunk::Y_AXIS_SIZE {
                for z in 0..chunk::Z_AXIS_SIZE {
                    for x in 0..chunk::X_AXIS_SIZE {
                        vec.push(IVec3::new(x as i32, y as i32, z as i32));
                    }
                }
            }
            vec
        };

        assert!(!merger_it.is_empty(), "Merger iterator must be non empty");

        merger_it.into_iter().enumerate().for_each(|(i, v)| {
            assert_eq!(&normal_it[i], &v, "Failed to match at index {}", i);
        })
    }

    #[test]
    fn merger_iterator_front() {
        let side = voxel::Side::Front;

        let merger_it = MergerIterator::new(side).collect::<Vec<_>>();
        let normal_it = {
            let mut vec = vec![];
            for z in 0..chunk::Z_AXIS_SIZE {
                for y in 0..chunk::Y_AXIS_SIZE {
                    for x in 0..chunk::X_AXIS_SIZE {
                        vec.push(IVec3::new(x as i32, y as i32, z as i32));
                    }
                }
            }
            vec
        };

        assert!(!merger_it.is_empty(), "Merger iterator must be non empty");

        merger_it.into_iter().enumerate().for_each(|(i, v)| {
            assert_eq!(&normal_it[i], &v, "Failed to match at index {}", i);
        })
    }

    #[test]
    fn merger_iterator_back() {
        let side = voxel::Side::Back;

        let merger_it = MergerIterator::new(side).collect::<Vec<_>>();
        let normal_it = {
            let mut vec = vec![];
            for z in 0..chunk::Z_AXIS_SIZE {
                for y in 0..chunk::Y_AXIS_SIZE {
                    for x in (0..=chunk::X_END).rev() {
                        vec.push(IVec3::new(x, y as i32, z as i32));
                    }
                }
            }
            vec
        };

        assert!(!merger_it.is_empty(), "Merger iterator must be non empty");

        merger_it.into_iter().enumerate().for_each(|(i, v)| {
            assert_eq!(&normal_it[i], &v, "Failed to match at index {}", i);
        })
    }

    #[test]
    fn axis_range_inc() {
        let range = AxisRange::new(0, 10);
        let v = range.collect::<Vec<_>>();

        assert_eq!(&v, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn axis_range_dec() {
        let range = AxisRange::new(10, 0);
        let v = range.collect::<Vec<_>>();

        assert_eq!(&v, &[10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0]);
    }
}
