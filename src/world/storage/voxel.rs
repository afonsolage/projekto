use bevy::prelude::*;

use crate::world::math;

use super::chunk;

pub const SIDE_COUNT: usize = 6;

pub type Kind = u16;
pub type FacesOcclusion = [bool; SIDE_COUNT];


#[derive(Clone, Copy, Debug)]
pub enum Side {
    Right = 0,
    Left = 1,
    Up = 2,
    Down = 3,
    Front = 4,
    Back = 5,
}

// impl Side {
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
// }

pub const SIDES: [Side; SIDE_COUNT] = [
    Side::Right,
    Side::Left,
    Side::Up,
    Side::Down,
    Side::Front,
    Side::Back,
];

pub fn get_side_normal(side: Side) -> [f32; 3] {
    match side {
        Side::Right => [1.0, 0.0, 0.0],
        Side::Left => [-1.0, 0.0, 0.0],
        Side::Up => [0.0, 1.0, 0.0],
        Side::Down => [0.0, -1.0, 0.0],
        Side::Front => [0.0, 0.0, 1.0],
        Side::Back => [0.0, 0.0, -1.0],
    }
}

pub fn get_side_dir(side: Side) -> IVec3 {
    match side {
        Side::Right => IVec3::X,
        Side::Left => -IVec3::X,
        Side::Up => IVec3::Y,
        Side::Down => -IVec3::Y,
        Side::Front => IVec3::Z,
        Side::Back => -IVec3::Z,
    }
}

pub fn to_local(world: Vec3) -> IVec3 {
    // First round world coords to integer.
    // This transform (1.1, -0.3, 17.5) into (1, -1, 17)
    let vec = math::floor(world);

    // Get the euclidean remainder
    // This transform (1, -1, 17) into (1, 15, 1)
    math::euclid_rem(vec, chunk::AXIS_SIZE as i32)
}

pub fn to_world(local: IVec3, chunk_local: IVec3) -> Vec3 {
    chunk::to_world(chunk_local) + local.as_f32()
}

#[cfg(test)]
mod tests {
    use bevy::math::{IVec3, Vec3};
    use rand::random;

    use super::chunk;

    #[test]
    fn to_world() {
        use super::*;

        const TEST_COUNT: usize = 1000;
        const MAG: f32 = 100.0;

        for _ in 0..TEST_COUNT {
            let base_chunk = IVec3::new(
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
            );

            let base_voxel = IVec3::new(
                (random::<f32>() * chunk::AXIS_SIZE as f32) as i32,
                (random::<f32>() * chunk::AXIS_SIZE as f32) as i32,
                (random::<f32>() * chunk::AXIS_SIZE as f32) as i32,
            );

            let chunk_world = base_chunk.as_f32() * chunk::AXIS_SIZE as f32;

            assert_eq!(
                chunk_world + base_voxel.as_f32(),
                super::to_world(base_voxel, base_chunk)
            );
        }
    }

    #[test]
    fn to_local() {
        assert_eq!(
            IVec3::new(0, 0, 0),
            super::to_local(Vec3::new(0.0, 0.0, 0.0))
        );
        assert_eq!(
            IVec3::new(1, 0, 0),
            super::to_local(Vec3::new(1.3, 0.0, 0.0))
        );
        assert_eq!(
            IVec3::new(15, 0, 0),
            super::to_local(Vec3::new(-0.3, 0.0, 0.0))
        );
        assert_eq!(
            IVec3::new(15, 1, 0),
            super::to_local(Vec3::new(-0.3, 17.3, 0.0))
        );
        assert_eq!(
            IVec3::new(1, 15, 1),
            super::to_local(Vec3::new(1.1, -0.3, 17.5))
        );

        const TEST_COUNT: usize = 1000;
        const MAG: f32 = 100.0;

        for _ in 0..TEST_COUNT {
            // Generate a valid voxel number between 0 and chunk::AXIS_SIZE
            let base = IVec3::new(
                (random::<f32>() * chunk::AXIS_SIZE as f32) as i32,
                (random::<f32>() * chunk::AXIS_SIZE as f32) as i32,
                (random::<f32>() * chunk::AXIS_SIZE as f32) as i32,
            );

            let sign = Vec3::new(
                if random::<bool>() { 1.0 } else { 1.0 },
                if random::<bool>() { 1.0 } else { 1.0 },
                if random::<bool>() { 1.0 } else { 1.0 },
            );

            // Generate some floating number between 0.0 and 0.9 just to simulate the fraction of world coordinates
            let frag = Vec3::new(
                random::<f32>() * 0.9,
                random::<f32>() * 0.9,
                random::<f32>() * 0.9,
            );

            // Compute a valid world coordinates using the base voxel, the sign and the floating number
            let world = Vec3::new(
                ((random::<f32>() * MAG * sign.x) as i32 * chunk::AXIS_SIZE as i32 + base.x) as f32,
                ((random::<f32>() * MAG * sign.y) as i32 * chunk::AXIS_SIZE as i32 + base.y) as f32,
                ((random::<f32>() * MAG * sign.z) as i32 * chunk::AXIS_SIZE as i32 + base.z) as f32,
            );

            assert_eq!(
                base,
                super::to_local(world + frag),
                "Failed to convert {:?} ({:?}) to local",
                world,
                frag
            );
        }
    }
}
