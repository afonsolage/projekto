use crate::world::{
    storage::{
        chunk::{self, Chunk},
        voxel::{self, VoxelFace},
    },
    terraformation::ChunkFacesOcclusion,
};
use bevy::prelude::*;

/**
  Checks if voxel is out of bounds, or is empty or is already merged or is fully occluded.
*/
#[inline]
fn should_skip_voxel(
    merged: &[bool],
    voxel: IVec3,
    side: voxel::Side,
    kind: voxel::Kind,
    occlusion: &ChunkFacesOcclusion,
) -> bool {
    kind.is_empty() || merged[chunk::to_index(voxel)] || occlusion.get(voxel).is_occluded(side)
}

/**
 Finds the furthest equal voxel from the given begin point, into the step direction.
*/
#[inline]
fn find_furthest_eq_voxel(
    begin: IVec3,
    step: IVec3,
    merged: &[bool],
    side: voxel::Side,
    chunk: &Chunk,
    occlusion: &ChunkFacesOcclusion,
    until: Option<IVec3>,
) -> IVec3 {
    perf_fn_scope!();

    let mut next_voxel = begin + step;

    while should_merge(begin, next_voxel, chunk, merged, side, occlusion) {
        if let Some(target) = until && target == next_voxel {
                return next_voxel;
            } else {
                next_voxel += step;
            }
    }

    next_voxel -= step;

    next_voxel
}

#[inline]
fn should_merge(
    voxel: IVec3,
    next_voxel: IVec3,
    chunk: &Chunk,
    merged: &[bool],
    side: voxel::Side,
    occlusion: &ChunkFacesOcclusion,
) -> bool {
    chunk::is_within_bounds(next_voxel)
        && !should_skip_voxel(
            merged,
            next_voxel,
            side,
            chunk.kinds.get(next_voxel),
            occlusion,
        )
        && chunk.kinds.get(voxel) == chunk.kinds.get(next_voxel)
        && chunk.lights.get_face_reflected_intensity(voxel, side)
            == chunk.lights.get_face_reflected_intensity(next_voxel, side)
}

/**
  The first tuple item is the outer most loop and the third item is the inner most.

  **Returns** a tuple indicating which direction the algorithm will walk in order to merge faces.
*/
#[inline]
fn get_side_walk_axis(side: voxel::Side) -> (IVec3, IVec3, IVec3) {
    match side {
        voxel::Side::Right => (IVec3::X, IVec3::Y, -IVec3::Z),
        voxel::Side::Left => (IVec3::X, IVec3::Y, IVec3::Z),
        voxel::Side::Up => (IVec3::Y, -IVec3::Z, IVec3::X),
        voxel::Side::Down => (IVec3::Y, IVec3::Z, IVec3::X),
        voxel::Side::Front => (IVec3::Z, IVec3::Y, IVec3::X),
        voxel::Side::Back => (IVec3::Z, IVec3::Y, -IVec3::X),
    }
}

/**
  This function returns a [`Box`] dyn iterator since it can return either [`Range`] or [`Rev<Iterator>`]

  **Returns** a boxed iterator to iterate over a given axis.
*/
#[inline]
fn get_axis_range(axis: IVec3) -> Box<dyn Iterator<Item = i32>> {
    match axis {
        _ if axis == IVec3::X => Box::new(0..chunk::X_AXIS_SIZE as i32),
        _ if axis == -IVec3::X => Box::new((0..=chunk::X_END).rev()),
        _ if axis == IVec3::Y => Box::new(0..chunk::Y_AXIS_SIZE as i32),
        _ if axis == -IVec3::Y => Box::new((0..=chunk::Y_END).rev()),
        _ if axis == IVec3::Z => Box::new(0..chunk::Z_AXIS_SIZE as i32),
        _ if axis == -IVec3::Z => Box::new((0..=chunk::Z_END).rev()),
        _ => unreachable!(),
    }
}

/**
 Converts a swizzled Vector in it's conventional (X, Y, Z) format

 **Returns** a [`IVec3`] with X, Y and Z elements in order.
*/
#[inline]
fn unswizzle(walk_axis: (IVec3, IVec3, IVec3), a: i32, b: i32, c: i32) -> IVec3 {
    walk_axis.0.abs() * a + walk_axis.1.abs() * b + walk_axis.2.abs() * c
}

/**
 Generates a list of voxels, based on v1, v2 and v3 inclusive, which was walked.
*/
#[inline]
fn calc_walked_voxels(
    v1: IVec3,
    v2: IVec3,
    v3: IVec3,
    perpendicular_axis: IVec3,
    current_axis: IVec3,
) -> Vec<IVec3> {
    perf_fn_scope!();

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

struct MergerIterator {
    walk_axis: (IVec3, IVec3, IVec3),
    a_range: Box<dyn Iterator<Item = i32>>,
    b_range: Box<dyn Iterator<Item = i32>>,
    c_range: Box<dyn Iterator<Item = i32>>,

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

/**
 Merge all faces which have the same voxel properties, like kind, lighting, AO and so on.

 The basic logic of function was inspired from [Greedy Mesh](https://0fps.net/2012/06/30/meshing-in-a-minecraft-game/).
 It was heavy modified to use a less mathematical and more logic approach.

 This function is very CPU intense so it should be run in a separated thread to avoid FPS drops.

 **Returns** a list of merged [`VoxelFace`]
*/
pub(super) fn merge(occlusion: ChunkFacesOcclusion, chunk: &Chunk) -> Vec<VoxelFace> {
    perf_fn_scope!();

    let mut faces_vertices = vec![];

    for side in voxel::SIDES {
        let walk_axis = get_side_walk_axis(side);
        let mut merged = vec![false; chunk::BUFFER_SIZE];

        for voxel in MergerIterator::new(side) {
            // Due to cache friendliness, the current axis is always the deepest on nested loop
            let current_axis = walk_axis.2;
            let perpendicular_axis = walk_axis.1;

            let kind = chunk.kinds.get(voxel);

            if should_skip_voxel(&merged, voxel, side, kind, &occlusion) {
                continue;
            }

            let light_intensity = chunk.lights.get_face_reflected_intensity(voxel, side);

            // Finds the furthest equal voxel on current axis
            let v1 = voxel;
            let v2 =
                find_furthest_eq_voxel(voxel, current_axis, &merged, side, chunk, &occlusion, None);

            // Finds the furthest equal voxel on perpendicular axis
            let perpendicular_step = perpendicular_axis;
            let mut v3 = v2 + perpendicular_step;

            // The loop walks all the way up on current_axis and than stepping one unit at time on perpendicular_axis.
            // This walk it'll be possible to find the next vertex (v3) which is be able to merge with v1 and v2
            let mut next_begin_voxel = v1 + perpendicular_step;
            while should_merge(voxel, next_begin_voxel, chunk, &merged, side, &occlusion) {
                let furthest = find_furthest_eq_voxel(
                    next_begin_voxel,
                    current_axis,
                    &merged,
                    side,
                    chunk,
                    &occlusion,
                    Some(v3),
                );

                if furthest == v3 {
                    v3 += perpendicular_step;
                    next_begin_voxel += perpendicular_step;
                } else {
                    break;
                }
            }

            // At this point, v3 is out-of-bounds or points to a voxel which can't be merged, so step-back one unit
            v3 -= perpendicular_step;

            // Flag walked voxels, making a perfect square from v1, v2 and v3, on the given axis.
            for voxel in calc_walked_voxels(v1, v2, v3, perpendicular_axis, current_axis) {
                merged[chunk::to_index(voxel)] = true;
            }

            // v4 can be inferred com v1, v2 and v3
            let v4 = v1 + (v3 - v2);

            faces_vertices.push(VoxelFace {
                vertices: [v1, v2, v3, v4],
                side,
                kind,
                light_intensity,
            })
        }
    }
    faces_vertices
}

#[cfg(test)]
mod tests {
    extern crate test;

    use super::*;
    use rand::prelude::*;
    use test::Bencher;

    #[bench]
    fn merge_faces_empty_chunk(b: &mut Bencher) {
        b.iter(|| {
            super::merge(ChunkFacesOcclusion::default(), &Chunk::default());
        });
    }

    #[bench]
    fn merge_faces_half_empty_chunk(b: &mut Bencher) {
        let mut chunk = Chunk::default();

        let mut rng = StdRng::seed_from_u64(53230);

        for i in 0..chunk::BUFFER_SIZE / 2 {
            chunk.kinds[i] = rng.gen_range(1u16..100).into();
        }

        b.iter(|| {
            super::merge(ChunkFacesOcclusion::default(), &chunk);
        });
    }

    #[bench]
    fn merge_faces_half_full_chunk(b: &mut Bencher) {
        let mut chunk = Chunk::default();

        let mut rng = StdRng::seed_from_u64(53230);

        for i in 0..chunk::BUFFER_SIZE {
            chunk.kinds[i] = rng.gen_range(1u16..100).into();
        }

        b.iter(|| {
            super::merge(ChunkFacesOcclusion::default(), &chunk);
        });
    }

    #[bench]
    fn merge_faces_worst_case(b: &mut Bencher) {
        let mut chunk = Chunk::default();

        for i in 0..chunk::BUFFER_SIZE {
            chunk.kinds[i] = ((i % u16::MAX as usize) as u16).into();
        }

        b.iter(|| {
            super::merge(ChunkFacesOcclusion::default(), &chunk);
        });
    }

    #[test]
    fn merge_right_faces() {
        /*
                        +-------------------+        +-------------------+
                     4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
                        +-------------------+        +-------- 2 -   ----+
                     3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
                        +-------------------+        +------------   ----+
               Y     2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
               |        +-------------------+        +----       --------+
               |     1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        Z -----+        +-------------------+        +----       --------+
                     0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
                        +-------------------+        +-------------------+

                     +    4   3   2   1   0

                       Merge direction (-Z, Y)[->, ^]
        */

        let mut chunk = Chunk::default();
        let kinds = &mut chunk.kinds;

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

        let merged = super::merge(ChunkFacesOcclusion::default(), &chunk)
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Right) //We care only for right faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<VoxelFace> = vec![
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
        /*
                        +-------------------+        +-------------------+
                     4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
                        +-------------------+        +-------- 2 -   ----+
                     3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
                        +-------------------+        +------------   ----+
        Y            2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        |               +-------------------+        +----       --------+
        |            1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        +----- Z        +-------------------+        +----       --------+
                     0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
                        +-------------------+        +-------------------+

                     +    0   1   2   3   4

                       Merge direction (Z, Y)[->, ^]
        */

        let mut chunk = Chunk::default();
        let kinds = &mut chunk.kinds;

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

        let merged = super::merge(ChunkFacesOcclusion::default(), &chunk)
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Left) //We care only for left faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<VoxelFace> = vec![
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
        /*
                        +-------------------+        +-------------------+
                     0  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
                        +-------------------+        +-------- 2 -   ----+
                     1  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
                        +-------------------+        +------------   ----+
        +----- X     2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        |               +-------------------+        +----       --------+
        |            3  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        Z               +-------------------+        +----       --------+
                     4  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
                        +-------------------+        +-------------------+

                     +    0   1   2   3   4

                       Merge direction (X, -Z)[->, ^]
        */

        let mut chunk = Chunk::default();
        let kinds = &mut chunk.kinds;

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

        let merged = super::merge(ChunkFacesOcclusion::default(), &chunk)
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Up) //We care only for Up faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<VoxelFace> = vec![
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
        /*
                        +-------------------+        +-------------------+
                     0  | 0 | 1 | 1 | 0 | 0 |        | 0 |       | 0 | 0 |
                        +-------------------+        +----   1   --------+
                     1  | 0 | 1 | 1 | 0 | 0 |        | 0 |       | 0 | 0 |
                        +-------------------+        +-------------------+
        +----- X     2  | 0 | 1 | 2 | 0 | 0 |   ->   | 0 | 1 | 2 | 0 | 0 |
        |               +-------------------+        +-------------------+
        |            3  | 0 | 1 | 1 | 0 | 0 |        | 0 |       | 0 | 0 |
        Z               +-------------------+        +----   1   --------+
                     4  | 0 | 1 | 1 | 0 | 0 |        | 0 |       | 0 | 0 |
                        +-------------------+        +-------------------+

                     +    0   1   2   3   4

                       Merge direction (X, -Z)[->, ^]
        */

        let mut chunk = Chunk::default();
        let kinds = &mut chunk.kinds;

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

        let merged = super::merge(ChunkFacesOcclusion::default(), &chunk)
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Up) //We care only for Up faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<VoxelFace> = vec![
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
        /*
                        +-------------------+        +-------------------+
                     4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
                        +-------------------+        +-------- 2 -   ----+
                     3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
                        +-------------------+        +------------   ----+
        Z            2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        |               +-------------------+        +----       --------+
        |            1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        +----- X        +-------------------+        +----       --------+
                     0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
                        +-------------------+        +-------------------+

                     +    0   1   2   3   4

                       Merge direction (X, Z)[->, ^]
        */

        let mut chunk = Chunk::default();
        let kinds = &mut chunk.kinds;

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

        let merged = super::merge(ChunkFacesOcclusion::default(), &chunk)
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Down) //We care only for Down faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<VoxelFace> = vec![
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
        /*
                        +-------------------+        +-------------------+
                     4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
                        +-------------------+        +-------- 2 -   ----+
                     3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
                        +-------------------+        +------------   ----+
        Y            2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
        |               +-------------------+        +----       --------+
        |            1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        +----- X        +-------------------+        +----       --------+
                     0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
                        +-------------------+        +-------------------+

                     +    0   1   2   3   4

                       Merge direction (X, Y)[->, ^]
        */

        let mut chunk = Chunk::default();
        let kinds = &mut chunk.kinds;

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

        let merged = super::merge(ChunkFacesOcclusion::default(), &chunk)
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Front) //We care only for Front faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<VoxelFace> = vec![
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
        /*
                        +-------------------+        +-------------------+
                     4  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   |   | 0 |
                        +-------------------+        +-------- 2 -   ----+
                     3  | 0 | 0 | 2 | 2 | 0 |        | 0 | 0 |   | 2 | 0 |
                        +-------------------+        +------------   ----+
               Y     2  | 0 | 1 | 1 | 2 | 0 |   ->   | 0 |       |   | 0 |
               |        +-------------------+        +----       --------+
               |     1  | 0 | 1 | 1 | 1 | 0 |        | 0 |   1   | 1 | 0 |
        X -----+        +-------------------+        +----       --------+
                     0  | 0 | 1 | 1 | 0 | 1 |        | 0 |       | 0 | 1 |
                        +-------------------+        +-------------------+

                     +    4   3   2   1   0

                       Merge direction (-X, Y)[->, ^]
        */

        let mut chunk = Chunk::default();
        let kinds = &mut chunk.kinds;

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

        let merged = super::merge(ChunkFacesOcclusion::default(), &chunk)
            .into_iter()
            .filter(|vf| vf.side == voxel::Side::Back) //We care only for Back faces here
            .collect::<Vec<_>>();

        let test_merged: Vec<VoxelFace> = vec![
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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
            voxel::VoxelFace {
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

        dbg!(&merged);

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
                        vec.push(IVec3::new(x as i32, y as i32, z as i32));
                    }
                }
            }
            vec
        };

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
                        vec.push(IVec3::new(x as i32, y as i32, z as i32));
                    }
                }
            }
            vec
        };

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
                        vec.push(IVec3::new(x as i32, y as i32, z as i32));
                    }
                }
            }
            vec
        };

        merger_it.into_iter().enumerate().for_each(|(i, v)| {
            assert_eq!(&normal_it[i], &v, "Failed to match at index {}", i);
        })
    }
}
