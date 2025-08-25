use std::hash::Hash;

use bevy::math::{IVec3, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::{chunk::ChunkStorage, coords::ChunkVoxel};

mod kind;
pub use kind::*;

pub const SIDE_COUNT: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum LightTy {
    Natural,
    Artificial,
}

impl LightTy {
    const fn offset(&self) -> u8 {
        match self {
            LightTy::Natural => 0xF,
            LightTy::Artificial => 0xF0,
        }
    }

    const fn shift(&self) -> usize {
        match self {
            LightTy::Natural => 0,
            LightTy::Artificial => 4,
        }
    }
}

#[derive(Hash, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Default, Deserialize, Serialize)]
pub struct Light(u8);

impl Light {
    pub const MAX_NATURAL_INTENSITY: u8 = 15;

    pub fn natural(intensity: u8) -> Self {
        let mut light = Light::default();
        light.set(LightTy::Natural, intensity);
        light
    }

    pub fn set(&mut self, ty: LightTy, intensity: u8) {
        self.0 = (self.0 & !ty.offset()) | (intensity << ty.shift());
    }

    #[inline]
    pub fn get(&self, ty: LightTy) -> u8 {
        (self.0 & ty.offset()) >> ty.shift()
    }

    pub fn get_greater_intensity(&self) -> u8 {
        std::cmp::max(self.get(LightTy::Artificial), self.get(LightTy::Natural))
    }
}

impl From<u8> for Light {
    fn from(v: u8) -> Self {
        Self(v)
    }
}

impl From<Light> for u8 {
    fn from(val: Light) -> Self {
        val.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Default, Serialize, Deserialize)]
pub enum Side {
    #[default]
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
    pub fn opposite(&self) -> Side {
        match self {
            Side::Right => Side::Left,
            Side::Left => Side::Right,
            Side::Up => Side::Down,
            Side::Down => Side::Up,
            Side::Front => Side::Back,
            Side::Back => Side::Front,
        }
    }
    pub fn index(&self) -> usize {
        match self {
            Side::Right => 0,
            Side::Left => 1,
            Side::Up => 2,
            Side::Down => 3,
            Side::Front => 4,
            Side::Back => 5,
        }
    }

    #[inline]
    pub fn normal(&self) -> Vec3 {
        match self {
            Side::Right => Vec3::X,
            Side::Left => -Vec3::X,
            Side::Up => Vec3::Y,
            Side::Down => -Vec3::Y,
            Side::Front => Vec3::Z,
            Side::Back => -Vec3::Z,
        }
    }

    pub fn dir(&self) -> IVec3 {
        match self {
            Side::Right => IVec3::X,
            Side::Left => -IVec3::X,
            Side::Up => IVec3::Y,
            Side::Down => -IVec3::Y,
            Side::Front => IVec3::Z,
            Side::Back => -IVec3::Z,
        }
    }

    #[inline]
    pub fn from_dir(dir: IVec3) -> Side {
        if dir == IVec3::X {
            Side::Right
        } else if dir == -IVec3::X {
            Side::Left
        } else if dir == IVec3::Y {
            Side::Up
        } else if dir == -IVec3::Y {
            Side::Down
        } else if dir == IVec3::Z {
            Side::Front
        } else if dir == -IVec3::Z {
            Side::Back
        } else {
            panic!("Invalid direction received: {dir:?}")
        }
    }
}

#[derive(Hash, Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Deserialize, Serialize)]
pub struct FacesOcclusion(u8);

const FULL_OCCLUDED_MASK: u8 = 0b0011_1111;

impl FacesOcclusion {
    pub fn fully_occluded() -> Self {
        Self(FULL_OCCLUDED_MASK)
    }

    pub fn set_all(&mut self, occluded: bool) {
        if occluded {
            self.0 = FULL_OCCLUDED_MASK;
        } else {
            self.0 = 0;
        }
    }

    pub fn is_fully_occluded(&self) -> bool {
        self.0 & FULL_OCCLUDED_MASK == FULL_OCCLUDED_MASK
    }

    pub fn is_occluded(&self, side: Side) -> bool {
        let mask = 1 << side as usize;
        self.0 & mask == mask
    }

    pub fn set(&mut self, side: Side, occluded: bool) {
        let mask = 1 << side as usize;
        if occluded {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }

    pub fn raw(&self) -> u8 {
        self.0
    }
}

impl From<[bool; 6]> for FacesOcclusion {
    fn from(v: [bool; 6]) -> Self {
        let mut result = Self::default();

        for side in SIDES {
            result.set(side, v[side as usize]);
        }

        result
    }
}

pub type ChunkFacesOcclusion = ChunkStorage<FacesOcclusion>;

impl ChunkFacesOcclusion {
    pub fn is_fully_occluded(&self) -> bool {
        self.all(FacesOcclusion::is_fully_occluded)
    }
}

/// Contains smoothed vertex light for each face
#[derive(Hash, Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
pub struct FacesSoftLight([u128; SIDE_COUNT]);

impl FacesSoftLight {
    pub fn new(soft_light: [[f32; 4]; SIDE_COUNT]) -> Self {
        let mut buffer = [0u128; SIDE_COUNT];
        for i in 0..SIDE_COUNT {
            buffer[i] = Self::from_array_f32(soft_light[i]);
        }
        Self(buffer)
    }

    pub fn with_intensity(intensity: u8) -> Self {
        Self::new([[intensity as f32; 4]; SIDE_COUNT])
    }

    pub fn set(&mut self, side: Side, light: [f32; 4]) {
        self.0[side as usize] = Self::from_array_f32(light);
    }

    pub fn get(&self, side: Side) -> [f32; 4] {
        Self::to_array_f32(self.0[side as usize])
    }

    #[inline]
    fn from_array_f32(array: [f32; 4]) -> u128 {
        (array[0].to_bits() as u128) << 96
            | (array[1].to_bits() as u128) << 64
            | (array[2].to_bits() as u128) << 32
            | array[3].to_bits() as u128
    }

    #[inline]
    fn to_array_f32(bits: u128) -> [f32; 4] {
        [
            f32::from_bits(((bits >> 96) & 0xFFFF_FFFF) as u32),
            f32::from_bits(((bits >> 64) & 0xFFFF_FFFF) as u32),
            f32::from_bits(((bits >> 32) & 0xFFFF_FFFF) as u32),
            f32::from_bits((bits & 0xFFFF_FFFF) as u32),
        ]
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub vertices: [ChunkVoxel; 4],
    pub side: Side,
    pub kind: Kind,
    pub light: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub tile_coord_start: Vec2,
    pub light: Vec3,
    // TODO: color
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::*;

    #[test]
    fn light() {
        let mut light = Light::default();

        let intensity = rand::rng().random_range(0..=15);

        light.set(LightTy::Artificial, intensity);

        assert_eq!(intensity, light.get(LightTy::Artificial));

        let intensity = rand::rng().random_range(0..=15);
        light.set(LightTy::Natural, intensity);

        assert_eq!(intensity, light.get(LightTy::Natural));

        let mut light = Light::default();
        light.set(LightTy::Natural, 3);
        light.set(LightTy::Artificial, 4);

        assert_eq!(light.get_greater_intensity(), 4);
    }

    #[test]
    fn faces_soft_light() {
        let faces_soft_light = FacesSoftLight::with_intensity(15);

        for side in SIDES {
            assert_eq!(faces_soft_light.get(side), [15.0; 4]);
        }

        let faces_soft_light = FacesSoftLight::default();

        for side in SIDES {
            assert_eq!(faces_soft_light.get(side), [0.0; 4]);
        }

        let soft_light = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
            [17.0, 18.0, 19.0, 20.0],
            [21.0, 22.0, 23.0, 24.0],
        ];

        let mut faces_soft_light = FacesSoftLight::new(soft_light);

        for (i, side) in SIDES.iter().enumerate() {
            assert_eq!(faces_soft_light.get(*side), soft_light[i]);
        }

        let light = [25.0, 26.0, 27.0, 28.0];
        faces_soft_light.set(Side::Up, light);

        assert_eq!(faces_soft_light.get(Side::Up), light);
    }

    #[test]
    fn faces_occlusion() {
        let mut occlusion = FacesOcclusion::default();
        assert!(!occlusion.is_fully_occluded());

        for side in super::SIDES {
            assert!(!occlusion.is_occluded(side));
        }

        occlusion.set(super::Side::Up, true);
        assert!(occlusion.is_occluded(super::Side::Up));

        occlusion.set(super::Side::Back, true);
        assert!(occlusion.is_occluded(super::Side::Back));

        for side in super::SIDES {
            occlusion.set(side, true);
        }

        assert!(occlusion.is_fully_occluded());

        for side in super::SIDES {
            assert!(occlusion.is_occluded(side));
        }

        occlusion.set(super::Side::Back, false);
        assert!(!occlusion.is_occluded(super::Side::Back));

        for side in super::SIDES {
            occlusion.set(side, false);
        }

        assert!(!occlusion.is_fully_occluded());

        for side in super::SIDES {
            assert!(!occlusion.is_occluded(side));
        }
    }
}
