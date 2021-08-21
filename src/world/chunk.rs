use bevy::prelude::*;

use super::math;

pub const AXIS_SIZE: usize = 16;
// const CHUNK_AXIS_OFFSET: usize = CHUNK_AXIS_SIZE / 2;
pub const BUFFER_SIZE: usize = AXIS_SIZE * AXIS_SIZE * AXIS_SIZE;

pub const X_MASK: usize = 0b_1111_0000_0000;
pub const Z_MASK: usize = 0b_0000_1111_0000;
pub const Y_MASK: usize = 0b_0000_0000_1111;

pub const X_SHIFT: usize = 8;
pub const Z_SHIFT: usize = 4;
pub const Y_SHIFT: usize = 0;

pub fn to_xyz(index: usize) -> (usize, usize, usize) {
    (
        (index & X_MASK) >> X_SHIFT,
        (index & Y_MASK) >> Y_SHIFT,
        (index & Z_MASK) >> Z_SHIFT,
    )
}

pub fn to_xyz_ivec3(index: usize) -> IVec3 {
    let (x, y, z) = to_xyz(index);
    IVec3::new(x as i32, y as i32, z as i32)
}

pub fn to_index(x: usize, y: usize, z: usize) -> usize {
    x << X_SHIFT | y << Y_SHIFT | z << Z_SHIFT
}

pub fn is_whitin_bounds(pos: IVec3) -> bool {
    math::is_within_cubic_bounds(pos, 0, AXIS_SIZE as i32 - 1)
}

pub fn to_world(local: IVec3) -> Vec3 {
    local.as_f32() * AXIS_SIZE as f32
}

pub fn to_local(world: Vec3) -> IVec3 {
    math::trunc(world) / AXIS_SIZE as i32
}

pub fn to_world_local(world: Vec3) -> Vec3 {
    to_world(to_local(world))
}

pub fn raycast(origin: Vec3, dir: Vec3) -> (Vec<IVec3>, Vec<Vec3>, Vec<IVec3>) {
    let mut visited_voxels = vec![];
    let mut visited_positions = vec![];
    let mut visited_normals = vec![];

    let mut current_pos = origin;
    let mut current_voxel = math::trunc(origin);
    let mut last_voxel = current_voxel;

    // Compute
    let grid_dir = math::to_grid_dir(dir);
    let tile_offset = IVec3::new(
        if dir.x >= 0.0 { 1 } else { 0 },
        if dir.y >= 0.0 { 1 } else { 0 },
        if dir.z >= 0.0 { 1 } else { 0 },
    );

    while is_whitin_bounds(current_voxel) {
        visited_voxels.push(current_voxel);
        visited_positions.push(current_pos);
        visited_normals.push(last_voxel - current_voxel);

        last_voxel = current_voxel;

        let next_voxel = current_voxel + tile_offset;
        let delta = (next_voxel.as_f32() - current_pos) / dir;
        let distance = if delta.x < delta.y && delta.x < delta.z {
            current_voxel.x += grid_dir.x;
            delta.x
        } else if delta.y < delta.x && delta.y < delta.z {
            current_voxel.y += grid_dir.y;
            delta.y
        } else {
            current_voxel.z += grid_dir.z;
            delta.z
        };

        current_pos += distance * dir * 1.01;
    }

    (visited_voxels, visited_positions, visited_normals)
}
