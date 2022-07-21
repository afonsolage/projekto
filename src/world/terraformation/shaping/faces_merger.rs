use crate::world::{
    query,
    storage::{
        chunk::{self, ChunkKind},
        voxel::{self, VoxelFace},
    },
    terraformation::ChunkFacesOcclusion,
};
use bevy::prelude::*;

/**
  Checks if voxel is out of bounds, or is empty or is already merged or is fully occluded.
*/
fn should_skip_voxel(
    merged: &Vec<usize>,
    voxel: IVec3,
    side: voxel::Side,
    kind: voxel::Kind,
    occlusion: &ChunkFacesOcclusion,
) -> bool {
    kind.is_empty() || merged[chunk::to_index(voxel)] == 1 || occlusion.get(voxel).is_occluded(side)
}

/**
 Finds the furthest equal voxel from the given begin point, into the step direction.
*/
fn find_furthest_eq_voxel(
    begin: IVec3,
    step: IVec3,
    merged: &Vec<usize>,
    side: voxel::Side,
    kinds: &ChunkKind,
    occlusion: &ChunkFacesOcclusion,
    until: Option<IVec3>,
) -> IVec3 {
    let kind = kinds.get(begin);
    let mut next_voxel = begin + step;

    while chunk::is_within_bounds(next_voxel) {
        if let Some(target) = until && target == next_voxel {
            return next_voxel;
        }

        let next_kind = kinds.get(next_voxel);

        if next_kind != kind || should_skip_voxel(merged, next_voxel, side, kind, occlusion) {
            break;
        } else {
            next_voxel += step;
        }
    }

    next_voxel -= step;

    next_voxel
}

/**
  The first tuple item is the outer most loop and the third item is the inner most.

  **Returns** a tuple indicating which direction the algorithm will walk in order to merge faces.
*/
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
fn unswizzle(walk_axis: (IVec3, IVec3, IVec3), a: i32, b: i32, c: i32) -> IVec3 {
    walk_axis.0.abs() * a + walk_axis.1.abs() * b + walk_axis.2.abs() * c
}

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

 The basic logic of function was based on [Greedy Mesh](https://0fps.net/2012/06/30/meshing-in-a-minecraft-game/).
 It was heavy modified to use a less mathematical and more logic approach (Yeah I don't understood those aliens letters).

 This function is very CPU intense so it should be run in a separated thread to avoid FPS drops.

 **Returns** a list of merged [`VoxelFace`]
*/
pub(super) fn merge(occlusion: ChunkFacesOcclusion, kinds: &ChunkKind) -> Vec<VoxelFace> {
    perf_fn_scope!();

    let mut faces_vertices = vec![];

    for side in voxel::SIDES {
        let walk_axis = get_side_walk_axis(side);
        let mut merged = vec![0; chunk::BUFFER_SIZE];

        for voxel in MergerIterator::new(side) {
            // Due to cache friendliness, the current axis is always the deepest on nested loop
            let current_axis = walk_axis.2;
            let perpendicular_axis = walk_axis.1;

            let kind = kinds.get(voxel);

            if should_skip_voxel(&merged, voxel, side, kind, &occlusion) {
                continue;
            }

            // Finds the furthest equal voxel on current axis
            let v1 = voxel;
            let v2 =
                find_furthest_eq_voxel(voxel, current_axis, &merged, side, kinds, &occlusion, None);

            // Finds the furthest equal voxel on perpendicular axis
            let perpendicular_step = perpendicular_axis;
            let mut v3 = v2 + perpendicular_step;

            let mut next_begin_voxel = v1 + perpendicular_step;
            while chunk::is_within_bounds(next_begin_voxel) {
                let next_kind = kinds.get(next_begin_voxel);

                if next_kind != kind
                    || should_skip_voxel(&merged, next_begin_voxel, side, next_kind, &occlusion)
                {
                    break;
                } else {
                    let furthest = find_furthest_eq_voxel(
                        next_begin_voxel,
                        current_axis,
                        &merged,
                        side,
                        kinds,
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
            }

            v3 -= perpendicular_step;
            let v4 = v1 + (v3 - v2);

            for voxel in calc_walked_voxels(v1, v2, v3, perpendicular_axis, current_axis) {
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let mut kinds = ChunkKind::default();
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

        let merged = super::merge(ChunkFacesOcclusion::default(), &kinds)
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
            },
        ];

        assert_eq!(&merged[0], &test_merged[0]);
        assert_eq!(&merged[1], &test_merged[1]);
        assert_eq!(&merged[2], &test_merged[2]);
        assert_eq!(&merged[3], &test_merged[3]);
        assert_eq!(&merged[4], &test_merged[4]);

        assert_eq!(merged.len(), 5);
    }

    #[test]
    fn calc_walked_voxels_right() {
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
    fn calc_walked_voxels_front() {
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
