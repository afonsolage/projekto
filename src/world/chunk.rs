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

// pub fn to_world_local(world: Vec3) -> Vec3 {
//     to_world(to_local(world))
// }
