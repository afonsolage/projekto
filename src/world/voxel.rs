use bevy::prelude::*;

#[derive(Clone, Copy, Debug)]
pub enum Side {
    Right = 0,
    Left = 1,
    Up = 2,
    Down = 3,
    Front = 4,
    Back = 5,
}

impl Side {
    // fn opposite(&self) -> VoxelSides {
    //     match self {
    //         VoxelSides::Right => VoxelSides::Left,
    //         VoxelSides::Left => VoxelSides::Right,
    //         VoxelSides::Up => VoxelSides::Down,
    //         VoxelSides::Down => VoxelSides::Up,
    //         VoxelSides::Front => VoxelSides::Back,
    //         VoxelSides::Back => VoxelSides::Front,
    //     }
    // }
}

pub const SIDES: [Side; 6] = [
    Side::Right,
    Side::Left,
    Side::Up,
    Side::Down,
    Side::Front,
    Side::Back,
];

pub fn get_side_normal(side: &Side) -> [f32; 3] {
    match side {
        Side::Right => [1.0, 0.0, 0.0],
        Side::Left => [-1.0, 0.0, 0.0],
        Side::Up => [0.0, 1.0, 0.0],
        Side::Down => [0.0, -1.0, 0.0],
        Side::Front => [0.0, 0.0, 1.0],
        Side::Back => [0.0, 0.0, -1.0],
    }
}

pub fn get_side_dir(side: &Side) -> IVec3 {
    match side {
        Side::Right => IVec3::X,
        Side::Left => -IVec3::X,
        Side::Up => IVec3::Y,
        Side::Down => -IVec3::Y,
        Side::Front => IVec3::Z,
        Side::Back => -IVec3::Z,
    }
}