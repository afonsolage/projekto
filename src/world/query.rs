use bevy::prelude::*;

use crate::world::{
    math,
    storage::{chunk, voxel},
};

/**
 An interator which produced a finite number of [`IVec3`] ranging from `begin` until `end` exclusive
 */
pub struct RangeIterator {
    begin: IVec3,
    end: IVec3,
    current: IVec3,
}

impl Iterator for RangeIterator {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        for x in self.current.x..self.end.x {
            for z in self.current.z..self.end.z {
                if let Some(y) = (self.current.y..self.end.y).next() {
                    self.current.y += 1;
                    return Some((x, y, z).into());
                } else {
                    self.current.z += 1;
                    self.current.y = self.begin.y;
                }
            }
            self.current.x += 1;
            self.current.z = self.begin.z;
            self.current.y = self.begin.y;
        }
        None
    }
}

pub fn range(begin: IVec3, end: IVec3) -> impl Iterator<Item = IVec3> {
    RangeIterator {
        begin,
        end,
        current: begin,
    }
}

/**
 An interator which produced a finite number of [`IVec3`] ranging from `begin` until `end` inclusive
 */
pub struct RangeInclusiveIterator {
    begin: IVec3,
    end: IVec3,
    current: IVec3,
}

impl Iterator for RangeInclusiveIterator {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        for x in self.current.x..=self.end.x {
            for z in self.current.z..=self.end.z {
                if let Some(y) = (self.current.y..=self.end.y).next() {
                    self.current.y += 1;
                    return Some((x, y, z).into());
                } else {
                    self.current.z += 1;
                    self.current.y = self.begin.y;
                }
            }
            self.current.x += 1;
            self.current.z = self.begin.z;
            self.current.y = self.begin.y;
        }
        None
    }
}

pub fn range_inclusive(begin: IVec3, end_inclusive: IVec3) -> impl Iterator<Item = IVec3> {
    RangeInclusiveIterator {
        begin,
        end: end_inclusive,
        current: begin,
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct RaycastHit {
    pub local: IVec3,
    pub position: Vec3,
    pub normal: IVec3,
}

pub fn raycast(origin: Vec3, dir: Vec3, range: f32) -> Vec<(RaycastHit, Vec<RaycastHit>)> {
    let mut result = vec![];

    let (hit_locals, hit_positions, hit_normals) = chunk_raycast(origin, dir, range);

    debug_assert_eq!(hit_locals.len(), hit_positions.len());
    debug_assert_eq!(hit_locals.len(), hit_normals.len());

    for (idx, local) in hit_locals.iter().enumerate() {
        let hit_position = hit_positions[idx];

        let chunk_hit = RaycastHit {
            local: *local,
            position: hit_position,
            normal: hit_normals[idx],
        };

        let remaining_range = range - hit_position.distance(origin);

        let (voxel_hit_locals, voxel_hit_positions, voxel_hit_normals) =
            voxel_raycast(hit_position, dir, remaining_range, *local);

        debug_assert_eq!(voxel_hit_locals.len(), voxel_hit_positions.len());
        debug_assert_eq!(voxel_hit_locals.len(), voxel_hit_normals.len());

        let mut voxels_hit = vec![];

        for (v_idx, v_local) in voxel_hit_locals.iter().enumerate() {
            voxels_hit.push(RaycastHit {
                local: *v_local,
                position: voxel_hit_positions[v_idx],
                normal: voxel_hit_normals[v_idx],
            })
        }

        result.push((chunk_hit, voxels_hit));
    }

    result
}

fn chunk_raycast(origin: Vec3, dir: Vec3, range: f32) -> (Vec<IVec3>, Vec<Vec3>, Vec<IVec3>) {
    let mut visited_locals = vec![];
    let mut visited_positions = vec![];
    let mut visited_normals = vec![];

    let mut current_pos = origin;
    let mut current_local = chunk::to_local(origin);
    let mut last_local = current_local;

    let grid_dir = dir.signum().as_ivec3();
    let step_dir = grid_dir.max(IVec3::ZERO);

    while current_pos.distance(origin) < range {
        visited_locals.push(current_local);
        visited_positions.push(current_pos);
        visited_normals.push(last_local - current_local);

        last_local = current_local;

        let next_local = current_local + step_dir;
        let delta = (chunk::to_world(next_local) - current_pos) / dir;
        let distance = if delta.x < delta.y && delta.x < delta.z {
            current_local.x += grid_dir.x;
            delta.x
        } else if delta.y < delta.x && delta.y < delta.z {
            current_local.y += grid_dir.y;
            delta.y
        } else {
            current_local.z += grid_dir.z;
            delta.z
        };

        current_pos += distance * dir * 1.01;
    }

    (visited_locals, visited_positions, visited_normals)
}

fn voxel_raycast(
    origin: Vec3,
    dir: Vec3,
    range: f32,
    chunk_local: IVec3,
) -> (Vec<IVec3>, Vec<Vec3>, Vec<IVec3>) {
    let mut visited_locals = vec![];
    let mut visited_positions = vec![];
    let mut visited_normals = vec![];

    let mut current_pos = origin;
    let mut current_local = voxel::to_local(origin);
    let mut last_local = current_local;

    let grid_dir = dir.signum().as_ivec3();
    let step_dir = grid_dir.max(IVec3::ZERO);

    while chunk::is_within_bounds(current_local) && current_pos.distance(origin) < range {
        visited_locals.push(current_local);
        visited_positions.push(current_pos);
        visited_normals.push(last_local - current_local);

        last_local = current_local;

        let next_local = current_local + step_dir;
        let delta = (voxel::to_world(next_local, chunk_local) - current_pos) / dir;

        let distance = match math::abs_min_element(delta) {
            math::Vec3Element::X => {
                current_local.x += grid_dir.x;
                delta.x
            }
            math::Vec3Element::Y => {
                current_local.y += grid_dir.y;
                delta.y
            }
            math::Vec3Element::Z => {
                current_local.z += grid_dir.z;
                delta.z
            }
        };

        current_pos += distance * dir * 1.01;
    }

    (visited_locals, visited_positions, visited_normals)
}

#[cfg(test)]
mod test {
    use std::vec;

    use bevy::math::{IVec3, Vec3};
    use rand::Rng;

    use crate::world::query::RaycastHit;

    #[test]
    fn range() {
        let items = super::range((0, 0, 0).into(), (1, 1, 1).into()).collect::<Vec<_>>();
        assert_eq!(items, vec![(0, 0, 0).into()]);

        let items = super::range((1, 1, 1).into(), (1, 1, 1).into()).collect::<Vec<_>>();
        assert_eq!(items, vec![]);

        for _ in 0..100 {
            let mut rnd = rand::thread_rng();

            let begin = IVec3::new(
                rnd.gen_range(-5..5),
                rnd.gen_range(-5..5),
                rnd.gen_range(-5..5),
            );
            let end = IVec3::new(
                rnd.gen_range(-5..5),
                rnd.gen_range(-5..5),
                rnd.gen_range(-5..5),
            );
            let items = super::range(begin, end).collect::<Vec<_>>();
            let mut loop_items = vec![];
            for x in begin.x..end.x {
                for z in begin.z..end.z {
                    for y in begin.y..end.y {
                        loop_items.push(IVec3::new(x, y, z));
                    }
                }
            }

            assert_eq!(items, loop_items, "Wrong values on range {} {}", begin, end);
        }
    }

    #[test]
    fn range_inclusive() {
        let items = super::range_inclusive((0, 0, 0).into(), (0, 0, 0).into()).collect::<Vec<_>>();
        assert_eq!(items, vec![(0, 0, 0).into()]);

        let items = super::range_inclusive((1, 2, 1).into(), (1, 1, 1).into()).collect::<Vec<_>>();
        assert_eq!(items, vec![]);

        for _ in 0..100 {
            let mut rnd = rand::thread_rng();

            let begin = IVec3::new(
                rnd.gen_range(-5..5),
                rnd.gen_range(-5..5),
                rnd.gen_range(-5..5),
            );
            let end_inclusive = IVec3::new(
                rnd.gen_range(-5..5),
                rnd.gen_range(-5..5),
                rnd.gen_range(-5..5),
            );
            let items = super::range_inclusive(begin, end_inclusive).collect::<Vec<_>>();
            let mut loop_items = vec![];
            for x in begin.x..=end_inclusive.x {
                for z in begin.z..=end_inclusive.z {
                    for y in begin.y..=end_inclusive.y {
                        loop_items.push(IVec3::new(x, y, z));
                    }
                }
            }

            assert_eq!(
                items, loop_items,
                "Wrong values on range {} {}",
                begin, end_inclusive
            );
        }
    }

    pub fn eq(vec_a: Vec3, vec_b: Vec3) -> bool {
        vec_a.abs_diff_eq(vec_b, f32::EPSILON)
    }

    fn assert_raycast(
        origin: Vec3,
        dir: Vec3,
        range: f32,
        ok_res: &[(RaycastHit, Vec<RaycastHit>)],
    ) {
        let res = super::raycast(origin, dir, range);

        assert_eq!(ok_res.len(), res.len());

        for (idx, (chunk_hit, voxels_hit)) in res.iter().enumerate() {
            assert_eq!(chunk_hit.local, ok_res[idx].0.local);
            assert!(eq(chunk_hit.position, ok_res[idx].0.position));
            assert_eq!(chunk_hit.local, ok_res[idx].0.local);

            for (v_idx, v_hit) in voxels_hit.iter().enumerate() {
                assert_eq!(v_hit.local, ok_res[idx].1[v_idx].local);
                assert!(eq(v_hit.position, ok_res[idx].1[v_idx].position));
                assert_eq!(v_hit.local, ok_res[idx].1[v_idx].local);
            }
        }
    }

    #[test]
    fn raycast_simple() {
        // Those parameters where extracted from game play.
        let origin = Vec3::new(1.5953689, 17.004368, 16.355797);
        let dir = Vec3::new(-0.32828045, -0.6908023, -0.6442237);
        let range = 20.0;
        let ok_res = vec![
            (
                RaycastHit {
                    local: IVec3::new(0, 1, 1),
                    position: Vec3::new(1.5953689, 17.004368, 16.355797),
                    normal: IVec3::new(0, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(1, 1, 0),
                        position: Vec3::new(1.5953689, 17.004368, 16.355797),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 0, 0),
                        position: Vec3::new(1.5932724, 16.999956, 16.351683),
                        normal: IVec3::new(0, 1, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, 1, 0),
                    position: Vec3::new(1.4122505, 16.619032, 15.996442),
                    normal: IVec3::new(0, 0, 1),
                },
                vec![RaycastHit {
                    local: IVec3::new(1, 0, 15),
                    position: Vec3::new(1.4122505, 16.619032, 15.996442),
                    normal: IVec3::new(0, 0, 0),
                }],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, 0, 0),
                    position: Vec3::new(1.1151347, 15.99381, 15.413376),
                    normal: IVec3::new(0, 1, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(1, 15, 15),
                        position: Vec3::new(1.1151347, 15.99381, 15.413376),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 15, 15),
                        position: Vec3::new(0.9988487, 15.749108, 15.185174),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 15, 14),
                        position: Vec3::new(0.903545, 15.54856, 14.998148),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 14, 14),
                        position: Vec3::new(0.64025354, 14.994514, 14.48146),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 14, 13),
                        position: Vec3::new(0.39246023, 14.473082, 13.995186),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 13, 13),
                        position: Vec3::new(0.16539602, 13.995269, 13.549591),
                        normal: IVec3::new(0, 1, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(-1, 0, 0),
                    position: Vec3::new(-0.011151314, 13.62376, 13.203131),
                    normal: IVec3::new(1, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(15, 13, 13),
                        position: Vec3::new(-0.011151314, 13.62376, 13.203131),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 13, 12),
                        position: Vec3::new(-0.11569681, 13.403765, 12.997969),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 12, 12),
                        position: Vec3::new(-0.3094911, 12.995962, 12.617663),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 12, 11),
                        position: Vec3::new(-0.62738454, 12.327018, 11.993823),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 11, 11),
                        position: Vec3::new(-0.78434277, 11.99673, 11.685805),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 11, 11),
                        position: Vec3::new(-1.0021565, 11.5383835, 11.258364),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 11, 10),
                        position: Vec3::new(-1.1351289, 11.258569, 10.9974165),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 10, 10),
                        position: Vec3::new(-1.2592337, 10.997415, 10.753871),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 10, 9),
                        position: Vec3::new(-1.6472292, 10.180954, 9.992461),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 9, 9),
                        position: Vec3::new(-1.7340814, 9.998191, 9.822021),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 9, 9),
                        position: Vec3::new(-2.002659, 9.433022, 9.294958),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 9, 8),
                        position: Vec3::new(-2.1544654, 9.113575, 8.99705),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 8, 8),
                        position: Vec3::new(-2.208978, 8.998864, 8.890074),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 8, 7),
                        position: Vec3::new(-2.667073, 8.034892, 7.9910994),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 7, 7),
                        position: Vec3::new(-2.68382, 7.999651, 7.9582343),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 7, 7),
                        position: Vec3::new(-3.003162, 7.327658, 7.331552),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 6, 7),
                        position: Vec3::new(-3.1604276, 6.996723, 7.022931),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 6, 6),
                        position: Vec3::new(-3.1722295, 6.971888, 6.9997706),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 5, 6),
                        position: Vec3::new(-3.638705, 5.990281, 6.0843506),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 5, 5),
                        position: Vec3::new(-3.6821177, 5.898927, 5.9991565),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 5, 5),
                        position: Vec3::new(-4.0031786, 5.2233167, 5.3691),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 4, 5),
                        position: Vec3::new(-4.1103635, 4.997767, 5.158758),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 4, 4),
                        position: Vec3::new(-4.192072, 4.825828, 4.9984126),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 3, 4),
                        position: Vec3::new(-4.5884433, 3.9917417, 4.220566),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 3, 3),
                        position: Vec3::new(-4.701962, 3.7528634, 3.9977944),
                        normal: IVec3::new(0, 0, 1),
                    },
                ],
            ),
        ];

        assert_raycast(origin, dir, range, &ok_res);
    }

    #[test]
    fn raycast_aligned_axis() {
        // Those parameters where extracted from game play.
        let origin = Vec3::new(0.58614147, 19.302107, 15.599731);
        let dir = Vec3::new(-0.0, -1.0, 0.000000012667444);
        let range = 20.0;
        let ok_res = vec![
            (
                RaycastHit {
                    local: IVec3::new(0, 1, 0),
                    position: Vec3::new(0.58614147, 19.302107, 15.599731),
                    normal: IVec3::new(0, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(0, 3, 15),
                        position: Vec3::new(0.58614147, 19.302107, 15.599731),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 2, 15),
                        position: Vec3::new(0.58614147, 18.996979, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 1, 15),
                        position: Vec3::new(0.58614147, 17.99003, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 0, 15),
                        position: Vec3::new(0.58614147, 16.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, 0, 0),
                    position: Vec3::new(0.58614147, 15.966979, 15.599731),
                    normal: IVec3::new(0, 1, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(0, 15, 15),
                        position: Vec3::new(0.58614147, 15.966979, 15.599731),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 14, 15),
                        position: Vec3::new(0.58614147, 14.99033, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 13, 15),
                        position: Vec3::new(0.58614147, 13.990097, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 12, 15),
                        position: Vec3::new(0.58614147, 12.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 11, 15),
                        position: Vec3::new(0.58614147, 11.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 10, 15),
                        position: Vec3::new(0.58614147, 10.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 9, 15),
                        position: Vec3::new(0.58614147, 9.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 8, 15),
                        position: Vec3::new(0.58614147, 8.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 7, 15),
                        position: Vec3::new(0.58614147, 7.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 6, 15),
                        position: Vec3::new(0.58614147, 6.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 5, 15),
                        position: Vec3::new(0.58614147, 5.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 4, 15),
                        position: Vec3::new(0.58614147, 4.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 3, 15),
                        position: Vec3::new(0.58614147, 3.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 2, 15),
                        position: Vec3::new(0.58614147, 2.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 1, 15),
                        position: Vec3::new(0.58614147, 1.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 0, 15),
                        position: Vec3::new(0.58614147, 0.990099, 15.599731),
                        normal: IVec3::new(0, 1, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, -1, 0),
                    position: Vec3::new(0.58614147, -0.15966892, 15.599731),
                    normal: IVec3::new(0, 1, 0),
                },
                vec![RaycastHit {
                    local: IVec3::new(0, 15, 15),
                    position: Vec3::new(0.58614147, -0.15966892, 15.599731),
                    normal: IVec3::new(0, 0, 0),
                }],
            ),
        ];

        assert_raycast(origin, dir, range, &ok_res);
    }

    #[test]
    fn raycast_many_chunks() {
        let origin = Vec3::new(-3.033941, 17.636923, 17.27036);
        let dir = Vec3::new(0.80696774, -0.381611, -0.4507508);
        let range = 100.0;

        let ok_res = vec![
            (
                RaycastHit {
                    local: IVec3::new(-1, 1, 1),
                    position: Vec3::new(-3.033941, 17.636923, 17.27036),
                    normal: IVec3::new(0, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(12, 1, 1),
                        position: Vec3::new(-3.033941, 17.636923, 17.27036),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 1, 1),
                        position: Vec3::new(-2.9996605, 17.620712, 17.251213),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 1, 0),
                        position: Vec3::new(-2.5454226, 17.405905, 16.997488),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 1, 0),
                        position: Vec3::new(-1.9945457, 17.145397, 16.689783),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 0, 0),
                        position: Vec3::new(-1.6840092, 16.998547, 16.516325),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 0, 0),
                        position: Vec3::new(-0.9931599, 16.671848, 16.130434),
                        normal: IVec3::new(-1, 0, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(-1, 1, 0),
                    position: Vec3::new(-0.73690295, 16.550665, 15.987296),
                    normal: IVec3::new(0, 0, 1),
                },
                vec![RaycastHit {
                    local: IVec3::new(15, 0, 15),
                    position: Vec3::new(-0.73690295, 16.550665, 15.987296),
                    normal: IVec3::new(0, 0, 0),
                }],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, 1, 0),
                    position: Vec3::new(0.0073690414, 16.198702, 15.571566),
                    normal: IVec3::new(-1, 0, 0),
                },
                vec![RaycastHit {
                    local: IVec3::new(0, 0, 15),
                    position: Vec3::new(0.0073690414, 16.198702, 15.571566),
                    normal: IVec3::new(0, 0, 0),
                }],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, 0, 0),
                    position: Vec3::new(0.43175262, 15.998013, 15.334517),
                    normal: IVec3::new(0, 1, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(0, 15, 15),
                        position: Vec3::new(0.43175262, 15.998013, 15.334517),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 15, 15),
                        position: Vec3::new(1.0056825, 15.726604, 15.013934),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 15, 14),
                        position: Vec3::new(1.0308778, 15.714689, 14.999861),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 15, 14),
                        position: Vec3::new(2.0096912, 15.251813, 14.453121),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 14, 14),
                        position: Vec3::new(2.5475085, 14.997482, 14.152711),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 14, 13),
                        position: Vec3::new(2.823637, 14.866902, 13.998473),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 14, 13),
                        position: Vec3::new(3.0017636, 14.782667, 13.898976),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 14, 13),
                        position: Vec3::new(4.009982, 14.305885, 13.335812),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 14, 12),
                        position: Vec3::new(4.617189, 14.018741, 12.996642),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 13, 12),
                        position: Vec3::new(4.6572146, 13.999812, 12.974285),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 13, 12),
                        position: Vec3::new(5.003428, 13.83609, 12.7809),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 13, 12),
                        position: Vec3::new(6.009966, 13.360104, 12.218675),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 13, 11),
                        position: Vec3::new(6.4053683, 13.17312, 11.997813),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 12, 11),
                        position: Vec3::new(6.7751136, 12.998269, 11.791284),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(7, 12, 11),
                        position: Vec3::new(7.002249, 12.890858, 11.664412),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 12, 11),
                        position: Vec3::new(8.009977, 12.414308, 11.101521),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 12, 10),
                        position: Vec3::new(8.193544, 12.327499, 10.998984),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 11, 10),
                        position: Vec3::new(8.893011, 11.996725, 10.60828),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 11, 10),
                        position: Vec3::new(9.00107, 11.945624, 10.547921),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 11, 9),
                        position: Vec3::new(9.991809, 11.477109, 9.994521),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 11, 9),
                        position: Vec3::new(10.000082, 11.473197, 9.9899),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 11, 9),
                        position: Vec3::new(11.009999, 10.995612, 9.425787),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 10, 9),
                        position: Vec3::new(11.0006275, 11.000044, 9.431022),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 10, 8),
                        position: Vec3::new(11.779991, 10.631487, 8.995689),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 10, 8),
                        position: Vec3::new(12.0022, 10.526405, 8.87157),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 10, 8),
                        position: Vec3::new(13.009978, 10.049832, 8.308652),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 9, 8),
                        position: Vec3::new(13.116409, 9.999501, 8.249203),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 9, 7),
                        position: Vec3::new(13.567012, 9.786413, 7.997508),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 9, 7),
                        position: Vec3::new(14.00433, 9.579608, 7.753234),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 9, 7),
                        position: Vec3::new(15.009956, 9.104052, 7.1915174),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 8, 7),
                        position: Vec3::new(15.232187, 8.99896, 7.0673847),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 8, 6),
                        position: Vec3::new(15.354031, 8.94134, 6.999326),
                        normal: IVec3::new(0, 0, 1),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(1, 0, 0),
                    position: Vec3::new(16.155684, 8.562245, 6.551546),
                    normal: IVec3::new(-1, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(0, 8, 6),
                        position: Vec3::new(16.155684, 8.562245, 6.551546),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 8, 6),
                        position: Vec3::new(17.008444, 8.158979, 6.0752172),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 8, 5),
                        position: Vec3::new(17.14445, 8.094663, 5.999248),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 7, 5),
                        position: Vec3::new(17.346628, 7.9990535, 5.8863163),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 7, 5),
                        position: Vec3::new(18.006535, 7.6869874, 5.5177107),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 7, 4),
                        position: Vec3::new(18.942648, 7.244304, 4.994823),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 7, 4),
                        position: Vec3::new(19.000574, 7.2169113, 4.962467),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 6, 4),
                        position: Vec3::new(19.46385, 6.997831, 4.703694),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 6, 4),
                        position: Vec3::new(20.005362, 6.7417526, 4.40122),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 6, 3),
                        position: Vec3::new(20.730839, 6.3986783, 3.995988),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 6, 3),
                        position: Vec3::new(21.002691, 6.2701206, 3.8441381),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 5, 3),
                        position: Vec3::new(21.579609, 5.9972987, 3.5218868),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 5, 3),
                        position: Vec3::new(22.004204, 5.7965097, 3.2847192),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 5, 2),
                        position: Vec3::new(22.519026, 5.5530524, 2.9971528),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(7, 5, 2),
                        position: Vec3::new(23.00481, 5.3233275, 2.7258067),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(7, 4, 2),
                        position: Vec3::new(23.695368, 4.9967666, 2.34008),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 4, 2),
                        position: Vec3::new(24.003046, 4.851267, 2.1682189),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 4, 1),
                        position: Vec3::new(24.307215, 4.7074265, 1.9983178),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 4, 1),
                        position: Vec3::new(25.006927, 4.376536, 1.6074766),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 3, 1),
                        position: Vec3::new(25.811125, 3.9962347, 1.1582729),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 3, 1),
                        position: Vec3::new(26.001888, 3.9060233, 1.0517172),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 3, 0),
                        position: Vec3::new(26.095402, 3.8618011, 0.9994828),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 3, 0),
                        position: Vec3::new(27.009047, 3.4297433, 0.48914534),
                        normal: IVec3::new(-1, 0, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(1, 0, -1),
                    position: Vec3::new(28.00204, 2.9601622, -0.06551552),
                    normal: IVec3::new(0, 0, 1),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(12, 2, 15),
                        position: Vec3::new(28.00204, 2.9601622, -0.06551552),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 2, 15),
                        position: Vec3::new(29.00998, 2.4835129, -0.6285234),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 2, 14),
                        position: Vec3::new(29.681675, 2.1658714, -1.0037148),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 2, 14),
                        position: Vec3::new(30.003183, 2.0138316, -1.1833009),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 1, 14),
                        position: Vec3::new(30.032724, 1.9998617, -1.1998018),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 1, 14),
                        position: Vec3::new(31.009672, 1.5378678, -1.7454993),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 1, 13),
                        position: Vec3::new(31.469854, 1.3202498, -2.0025449),
                        normal: IVec3::new(0, 0, 1),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(2, 0, -1),
                    position: Vec3::new(32.039978, 1.0506413, -2.321001),
                    normal: IVec3::new(-1, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(0, 1, 13),
                        position: Vec3::new(32.039978, 1.0506413, -2.321001),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 0, 13),
                        position: Vec3::new(32.148136, 0.9994936, -2.3814156),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 0, 13),
                        position: Vec3::new(33.00852, 0.59262305, -2.8620024),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 0, 12),
                        position: Vec3::new(33.25804, 0.47462434, -3.00138),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 0, 12),
                        position: Vec3::new(34.00742, 0.120247126, -3.419963),
                        normal: IVec3::new(-1, 0, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(2, -1, -1),
                    position: Vec3::new(34.283916, -0.0105063915, -3.5744061),
                    normal: IVec3::new(0, 1, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(2, 15, 12),
                        position: Vec3::new(34.283916, -0.0105063915, -3.5744061),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 15, 12),
                        position: Vec3::new(35.00716, -0.35252503, -3.9783912),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 15, 11),
                        position: Vec3::new(35.046234, -0.37100226, -4.000216),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 15, 11),
                        position: Vec3::new(36.009537, -0.8265437, -4.538292),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 14, 11),
                        position: Vec3::new(36.38, -1.0017345, -4.7452235),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 14, 10),
                        position: Vec3::new(36.840683, -1.2195883, -5.0025477),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 14, 10),
                        position: Vec3::new(37.001595, -1.295682, -5.0924277),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 14, 10),
                        position: Vec3::new(38.009983, -1.7725443, -5.6556873),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 13, 10),
                        position: Vec3::new(38.495777, -2.0022745, -5.92704),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 13, 9),
                        position: Vec3::new(38.6277, -2.064661, -6.0007296),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(7, 13, 9),
                        position: Vec3::new(39.003723, -2.24248, -6.210766),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 13, 9),
                        position: Vec3::new(40.009964, -2.7183256, -6.772825),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 13, 8),
                        position: Vec3::new(40.42074, -2.912578, -7.0022717),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 12, 8),
                        position: Vec3::new(40.607452, -3.0008743, -7.106565),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 12, 8),
                        position: Vec3::new(41.003925, -3.1883645, -7.3280244),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 12, 8),
                        position: Vec3::new(42.00996, -3.6641135, -7.889969),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 12, 7),
                        position: Vec3::new(42.208916, -3.7581987, -8.001101),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 11, 7),
                        position: Vec3::new(42.72535, -4.002418, -8.289567),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 11, 7),
                        position: Vec3::new(43.002747, -4.133598, -8.444513),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 11, 6),
                        position: Vec3::new(44.007164, -4.6085825, -9.005555),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 11, 6),
                        position: Vec3::new(43.999928, -4.6051607, -9.0015135),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 10, 6),
                        position: Vec3::new(44.843216, -5.003948, -9.472553),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 10, 6),
                        position: Vec3::new(45.001568, -5.078832, -9.561005),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 10, 5),
                        position: Vec3::new(45.79535, -5.4542074, -10.00439),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 10, 5),
                        position: Vec3::new(46.002045, -5.551954, -10.119845),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 9, 5),
                        position: Vec3::new(46.958973, -6.0044804, -10.65436),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 9, 5),
                        position: Vec3::new(47.000412, -6.024076, -10.6775055),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 9, 4),
                        position: Vec3::new(47.583538, -6.299834, -11.003225),
                        normal: IVec3::new(0, 0, 1),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(3, -1, -1),
                    position: Vec3::new(48.13716, -6.561637, -11.312462),
                    normal: IVec3::new(-1, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(0, 9, 4),
                        position: Vec3::new(48.13716, -6.561637, -11.312462),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 9, 4),
                        position: Vec3::new(49.00863, -6.973749, -11.79924),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 8, 4),
                        position: Vec3::new(49.064693, -7.0002627, -11.830557),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 8, 3),
                        position: Vec3::new(49.37108, -7.1451497, -12.001695),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 8, 3),
                        position: Vec3::new(50.00629, -7.4455376, -12.356506),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 8, 3),
                        position: Vec3::new(51.009937, -7.920157, -12.917117),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 8, 2),
                        position: Vec3::new(51.159805, -7.9910283, -13.000829),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 7, 2),
                        position: Vec3::new(51.178967, -8.00009, -13.011532),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 7, 2),
                        position: Vec3::new(52.00821, -8.392235, -13.474726),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 7, 1),
                        position: Vec3::new(52.958, -8.841385, -14.005253),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 7, 1),
                        position: Vec3::new(53.00042, -8.861445, -14.028948),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 6, 1),
                        position: Vec3::new(53.29634, -9.001386, -14.1942425),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 6, 1),
                        position: Vec3::new(54.00704, -9.33747, -14.591218),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 6, 0),
                        position: Vec3::new(54.74619, -9.687011, -15.004087),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(7, 6, 0),
                        position: Vec3::new(55.002537, -9.808237, -15.147277),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(7, 5, 0),
                        position: Vec3::new(55.4121, -10.001918, -15.376048),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 5, 0),
                        position: Vec3::new(56.00588, -10.282712, -15.707716),
                        normal: IVec3::new(-1, 0, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(3, -1, -2),
                    position: Vec3::new(56.61306, -10.569847, -16.046875),
                    normal: IVec3::new(0, 0, 1),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(8, 5, 15),
                        position: Vec3::new(56.61306, -10.569847, -16.046875),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 5, 15),
                        position: Vec3::new(57.00387, -10.754659, -16.265171),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 4, 15),
                        position: Vec3::new(57.527863, -11.002454, -16.557861),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 4, 15),
                        position: Vec3::new(58.004723, -11.227958, -16.824223),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 4, 14),
                        position: Vec3::new(58.32256, -11.378262, -17.001759),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 4, 14),
                        position: Vec3::new(59.006775, -11.701823, -17.383944),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 3, 14),
                        position: Vec3::new(59.643616, -12.002982, -17.739666),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 3, 14),
                        position: Vec3::new(60.003563, -12.1732, -17.940723),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 3, 13),
                        position: Vec3::new(60.110744, -12.223886, -18.000593),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 3, 13),
                        position: Vec3::new(61.008892, -12.648615, -18.502275),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 2, 13),
                        position: Vec3::new(61.759373, -13.003514, -18.921474),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 2, 12),
                        position: Vec3::new(61.90136, -13.07066, -19.000786),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 2, 12),
                        position: Vec3::new(62.000988, -13.117773, -19.056435),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 2, 12),
                        position: Vec3::new(63.00999, -13.594925, -19.620037),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 2, 11),
                        position: Vec3::new(63.697033, -13.919823, -20.0038),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(15, 1, 11),
                        position: Vec3::new(63.868275, -14.000802, -20.099451),
                        normal: IVec3::new(0, 1, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(4, -1, -2),
                    position: Vec3::new(64.07387, -14.098026, -20.214285),
                    normal: IVec3::new(-1, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(0, 1, 11),
                        position: Vec3::new(64.07387, -14.098026, -20.214285),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 1, 11),
                        position: Vec3::new(65.00926, -14.540369, -20.73677),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 1, 10),
                        position: Vec3::new(65.48523, -14.7654505, -21.002632),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 0, 10),
                        position: Vec3::new(65.986176, -15.002345, -21.282448),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 0, 10),
                        position: Vec3::new(66.00014, -15.008948, -21.290247),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 0, 10),
                        position: Vec3::new(67.01, -15.486506, -21.854328),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 0, 9),
                        position: Vec3::new(67.2734, -15.611067, -22.001457),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 0, 9),
                        position: Vec3::new(68.00726, -15.958109, -22.411375),
                        normal: IVec3::new(-1, 0, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(4, -2, -2),
                    position: Vec3::new(68.13606, -16.01902, -22.483322),
                    normal: IVec3::new(0, 1, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(4, 15, 9),
                        position: Vec3::new(68.13606, -16.01902, -22.483322),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 15, 9),
                        position: Vec3::new(69.00864, -16.431658, -22.97072),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 15, 8),
                        position: Vec3::new(69.06158, -16.456694, -23.000294),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 15, 8),
                        position: Vec3::new(70.009384, -16.904907, -23.529715),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 14, 8),
                        position: Vec3::new(70.21248, -17.000952, -23.64316),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 14, 7),
                        position: Vec3::new(70.85771, -17.306078, -24.003569),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(7, 14, 7),
                        position: Vec3::new(71.00142, -17.374039, -24.083841),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 14, 7),
                        position: Vec3::new(72.00999, -17.850985, -24.6472),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 13, 7),
                        position: Vec3::new(72.328255, -18.00149, -24.824974),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 13, 6),
                        position: Vec3::new(72.64473, -18.15115, -25.00175),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 13, 6),
                        position: Vec3::new(73.003555, -18.320835, -25.20218),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 13, 6),
                        position: Vec3::new(74.009964, -18.79676, -25.764334),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 13, 5),
                        position: Vec3::new(74.43609, -18.998274, -26.002357),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 12, 5),
                        position: Vec3::new(74.43977, -19.000017, -26.004417),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 12, 5),
                        position: Vec3::new(75.0056, -19.267595, -26.320475),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 12, 5),
                        position: Vec3::new(76.00994, -19.742544, -26.881475),
                        normal: IVec3::new(-1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 12, 4),
                        position: Vec3::new(76.22425, -19.843891, -27.001184),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 11, 4),
                        position: Vec3::new(76.55766, -20.00156, -27.187422),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 11, 4),
                        position: Vec3::new(77.004425, -20.212831, -27.43697),
                        normal: IVec3::new(-1, 0, 0),
                    },
                ],
            ),
        ];

        assert_raycast(origin, dir, range, &ok_res);
    }

    #[test]
    fn raycast_neg_dir() {
        let origin = Vec3::new(17.44164, 4.6248555, 6.514827);
        let dir = Vec3::new(-0.7835956, -0.4692715, -0.407139);
        let range = 20.0;
        let ok_res = vec![
            (
                RaycastHit {
                    local: IVec3::new(1, 0, 0),
                    position: Vec3::new(17.44164, 4.6248555, 6.514827),
                    normal: IVec3::new(0, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(1, 4, 6),
                        position: Vec3::new(17.44164, 4.6248555, 6.514827),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 4, 6),
                        position: Vec3::new(16.995584, 4.3577256, 6.2830653),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 4, 5),
                        position: Vec3::new(16.445337, 4.0281997, 5.9971695),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(0, 3, 5),
                        position: Vec3::new(16.397778, 3.999718, 5.972459),
                        normal: IVec3::new(0, 1, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, 0, 0),
                    position: Vec3::new(15.985583, 3.7528672, 5.7582917),
                    normal: IVec3::new(1, 0, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(15, 3, 5),
                        position: Vec3::new(15.985583, 3.7528672, 5.7582917),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 3, 5),
                        position: Vec3::new(14.990144, 3.1567292, 5.2410836),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 2, 5),
                        position: Vec3::new(14.725819, 2.9984326, 5.103746),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(14, 2, 4),
                        position: Vec3::new(14.524148, 2.8776586, 4.9989624),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 2, 4),
                        position: Vec3::new(13.994759, 2.5606234, 4.723903),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(13, 1, 4),
                        position: Vec3::new(13.049261, 1.9943938, 4.2326436),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 1, 4),
                        position: Vec3::new(12.999507, 1.9645978, 4.206793),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(12, 1, 3),
                        position: Vec3::new(12.597526, 1.7238634, 3.997932),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 1, 3),
                        position: Vec3::new(11.994024, 1.3624451, 3.6843662),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(11, 0, 3),
                        position: Vec3::new(11.382756, 0.99637556, 3.366765),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 0, 3),
                        position: Vec3::new(10.996172, 0.7648623, 3.1659045),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(10, 0, 2),
                        position: Vec3::new(10.673673, 0.5717273, 2.9983408),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(9, 0, 2),
                        position: Vec3::new(9.993263, 0.16425082, 2.6448152),
                        normal: IVec3::new(1, 0, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, -1, 0),
                    position: Vec3::new(9.656331, -0.037528753, 2.4697518),
                    normal: IVec3::new(0, 1, 0),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(9, 15, 2),
                        position: Vec3::new(9.656331, -0.037528753, 2.4697518),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 15, 2),
                        position: Vec3::new(8.993437, -0.43451595, 2.1253266),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 15, 1),
                        position: Vec3::new(8.749816, -0.58041286, 1.9987468),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(8, 14, 1),
                        position: Vec3::new(8.042177, -1.0041959, 1.6310735),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(7, 14, 1),
                        position: Vec3::new(7.999578, -1.0297072, 1.60894),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 14, 1),
                        position: Vec3::new(6.990004, -1.6343101, 1.0843878),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 14, 0),
                        position: Vec3::new(6.825964, -1.7325488, 0.9991561),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(6, 13, 0),
                        position: Vec3::new(6.3749046, -2.0026746, 0.76479566),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(5, 13, 0),
                        position: Vec3::new(5.996251, -2.2294388, 0.56805557),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 13, 0),
                        position: Vec3::new(4.9900374, -2.8320293, 0.045249164),
                        normal: IVec3::new(1, 0, 0),
                    },
                ],
            ),
            (
                RaycastHit {
                    local: IVec3::new(0, -1, -1),
                    position: Vec3::new(4.8554163, -2.9126499, -0.024697542),
                    normal: IVec3::new(0, 0, 1),
                },
                vec![
                    RaycastHit {
                        local: IVec3::new(4, 13, 15),
                        position: Vec3::new(4.8554163, -2.9126499, -0.024697542),
                        normal: IVec3::new(0, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(4, 12, 15),
                        position: Vec3::new(4.7080994, -3.0008736, -0.10124019),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 12, 15),
                        position: Vec3::new(3.992919, -3.4291732, -0.4728321),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(3, 11, 15),
                        position: Vec3::new(3.0302134, -4.005708, -0.9730327),
                        normal: IVec3::new(0, 1, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 11, 15),
                        position: Vec3::new(2.999698, -4.023983, -0.9888879),
                        normal: IVec3::new(1, 0, 0),
                    },
                    RaycastHit {
                        local: IVec3::new(2, 11, 14),
                        position: Vec3::new(2.9780972, -4.036919, -1.0001111),
                        normal: IVec3::new(0, 0, 1),
                    },
                    RaycastHit {
                        local: IVec3::new(1, 11, 14),
                        position: Vec3::new(1.9902191, -4.628529, -1.5133908),
                        normal: IVec3::new(1, 0, 0),
                    },
                ],
            ),
        ];

        assert_raycast(origin, dir, range, &ok_res);
    }
}
