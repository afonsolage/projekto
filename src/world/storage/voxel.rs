use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;
use std::ops::{Index, IndexMut};

use crate::world::math;

use super::chunk;
use super::chunk::ChunkStorageType;

pub const SIDE_COUNT: usize = 6;

#[derive(Deserialize)]
pub struct KindDescription {
    pub name: String,
    pub id: u16,
    pub color: (f32, f32, f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default, Deserialize, Serialize)]
pub struct Kind(u16);

impl From<u16> for Kind {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl Kind {
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl ChunkStorageType for Kind {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Right = 0,
    Left = 1,
    Up = 2,
    Down = 3,
    Front = 4,
    Back = 5,
}

pub const SIDES: [Side; SIDE_COUNT] = [
    Side::Right,
    Side::Left,
    Side::Up,
    Side::Down,
    Side::Front,
    Side::Back,
];

impl Side {
    pub fn get_side_normal(&self) -> Vec3 {
        match self {
            Side::Right => Vec3::new(1.0, 0.0, 0.0),
            Side::Left => Vec3::new(-1.0, 0.0, 0.0),
            Side::Up => Vec3::new(0.0, 1.0, 0.0),
            Side::Down => Vec3::new(0.0, -1.0, 0.0),
            Side::Front => Vec3::new(0.0, 0.0, 1.0),
            Side::Back => Vec3::new(0.0, 0.0, -1.0),
        }
    }

    pub fn get_side_dir(&self) -> IVec3 {
        match self {
            Side::Right => IVec3::X,
            Side::Left => -IVec3::X,
            Side::Up => IVec3::Y,
            Side::Down => -IVec3::Y,
            Side::Front => IVec3::Z,
            Side::Back => -IVec3::Z,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct FacesOcclusion([bool; SIDE_COUNT]);

impl FacesOcclusion {
    pub fn set_all(&mut self, occluded: bool) {
        self.0.fill(occluded)
    }

    #[cfg(test)]
    pub fn is_fully_occluded(&self) -> bool {
        return self.0.iter().all(|&b| b);
    }
}

impl Index<usize> for FacesOcclusion {
    type Output = bool;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for FacesOcclusion {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl From<[bool; 6]> for FacesOcclusion {
    fn from(v: [bool; 6]) -> Self {
        Self(v)
    }
}

impl ChunkStorageType for FacesOcclusion {}

#[derive(Debug, PartialEq, Eq)]
pub struct VoxelFace {
    pub vertices: [IVec3; 4],
    pub side: Side,
    //TODO: light and color
}

#[derive(Debug, Default, PartialEq)]
pub struct VoxelVertex {
    pub position: Vec3,
    pub normal: Vec3,
    //TODO: light and color
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
    use ron::de::from_reader;

    use crate::world::storage::voxel::KindDescription;

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

    #[test]
    fn load_kind_descriptions() {
        let input_path = format!(
            "{}/assets/voxels/kind_descriptions.ron",
            env!("CARGO_MANIFEST_DIR")
        );
        let f = std::fs::File::open(&input_path).expect("Failed opening kind descriptions file");

        let _: Vec<KindDescription> = from_reader(f).unwrap();
    }
}
