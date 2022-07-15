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

#[cfg(test)]
pub fn range(begin: IVec3, end: IVec3) -> impl Iterator<Item = IVec3> {
    RangeIterator {
        begin,
        end,
        current: begin,
    }
}

/**
 An iterator which produced a finite number of [`IVec3`] ranging from `begin` until `end` inclusive
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

    use rand::Rng;

    use super::*;

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

}
