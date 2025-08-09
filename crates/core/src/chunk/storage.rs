#![allow(unused)]
use crate::{
    chunk::{
        self, BUFFER_SIZE, Chunk,
        sub_chunk::{self, ChunkPack},
    },
    voxel::{self, Voxel},
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

const SUB_CHUNKS_X: usize = super::X_AXIS_SIZE / super::sub_chunk::X_AXIS_SIZE;
const SUB_CHUNKS_Y: usize = super::Y_AXIS_SIZE / super::sub_chunk::Y_AXIS_SIZE;
const SUB_CHUNKS_Z: usize = super::Z_AXIS_SIZE / super::sub_chunk::Z_AXIS_SIZE;

const SUB_CHUNKS_BUFFER_SIZE: usize = SUB_CHUNKS_X * SUB_CHUNKS_Y * SUB_CHUNKS_Z;

const X_SHIFT: usize = (SUB_CHUNKS_Z.ilog2() + Z_SHIFT as u32) as usize;
const Z_SHIFT: usize = SUB_CHUNKS_Y.ilog2() as usize;
const Y_SHIFT: usize = 0;

const X_MASK: usize = (SUB_CHUNKS_X - 1) << X_SHIFT;
const Z_MASK: usize = (SUB_CHUNKS_Z - 1) << Z_SHIFT;
const Y_MASK: usize = SUB_CHUNKS_Y - 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SubChunkStorage<T>(Vec<ChunkPack<T>>);

impl<T> SubChunkStorage<T> {
    #[inline]
    fn to_index(voxel: Voxel) -> usize {
        (voxel.x << X_SHIFT | voxel.z << Z_SHIFT | voxel.y << Y_SHIFT) as usize
    }
}

impl<T> SubChunkStorage<T>
where
    T: Default + Copy,
{
    fn new() -> Self {
        SubChunkStorage(vec![ChunkPack::default(); SUB_CHUNKS_BUFFER_SIZE])
    }
}

impl<T> Default for SubChunkStorage<T>
where
    T: Default + Copy,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> std::ops::Index<Voxel> for SubChunkStorage<T> {
    type Output = ChunkPack<T>;

    fn index(&self, voxel: Voxel) -> &Self::Output {
        &self.0[Self::to_index(voxel)]
    }
}

impl<T> std::ops::IndexMut<Voxel> for SubChunkStorage<T> {
    fn index_mut(&mut self, voxel: Voxel) -> &mut Self::Output {
        &mut self.0[Self::to_index(voxel)]
    }
}

pub trait ChunkStorageType:
    Clone + Copy + core::fmt::Debug + Default + PartialEq + Eq + PartialOrd + std::hash::Hash
{
}

impl ChunkStorageType for u8 {}
impl ChunkStorageType for u16 {}
impl ChunkStorageType for voxel::Kind {}
impl ChunkStorageType for voxel::Light {}
impl ChunkStorageType for voxel::FacesOcclusion {}
impl ChunkStorageType for voxel::FacesSoftLight {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkStorage<T>(SubChunkStorage<T>);

impl<T: ChunkStorageType> PartialEq for ChunkStorage<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: ChunkStorageType> std::default::Default for ChunkStorage<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T: ChunkStorageType> ChunkStorage<T> {
    pub fn get(&self, voxel: Voxel) -> T {
        let sub_chunk = voxel
            / IVec3::new(
                sub_chunk::X_AXIS_SIZE as i32,
                sub_chunk::Y_AXIS_SIZE as i32,
                sub_chunk::Z_AXIS_SIZE as i32,
            );
        let sub_voxel = voxel
            % IVec3::new(
                sub_chunk::X_AXIS_SIZE as i32,
                sub_chunk::Y_AXIS_SIZE as i32,
                sub_chunk::Z_AXIS_SIZE as i32,
            );
        self.0[sub_chunk].get(sub_voxel)
    }

    pub fn set(&mut self, voxel: Voxel, value: T) {
        let sub_chunk = voxel
            / IVec3::new(
                sub_chunk::X_AXIS_SIZE as i32,
                sub_chunk::Y_AXIS_SIZE as i32,
                sub_chunk::Z_AXIS_SIZE as i32,
            );
        let sub_voxel = voxel
            % IVec3::new(
                sub_chunk::X_AXIS_SIZE as i32,
                sub_chunk::Y_AXIS_SIZE as i32,
                sub_chunk::Z_AXIS_SIZE as i32,
            );
        self.0[sub_chunk].set(sub_voxel, value);
    }

    pub fn is_default(&self) -> bool {
        self.0
            .0
            .iter()
            .all(|pack| matches!(pack, ChunkPack::Single(_)))
    }

    pub fn all<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> bool + Copy,
    {
        self.0.0.iter().all(|pack| pack.all(f))
    }
}

pub trait GetChunkStorage<'a, T: ChunkStorageType + 'a>:
    Fn(Chunk) -> Option<&'a ChunkStorage<T>> + Copy
{
}

impl<'a, T: ChunkStorageType + 'a, F: Copy> GetChunkStorage<'a, T> for F where
    F: Fn(Chunk) -> Option<&'a ChunkStorage<T>>
{
}

pub trait GetChunkStorageMut<'a, T: ChunkStorageType + 'a>:
    FnMut(Chunk) -> Option<&'a mut ChunkStorage<T>> + Copy
{
}

impl<'a, T: ChunkStorageType + 'a, F: Copy> GetChunkStorageMut<'a, T> for F where
    F: FnMut(Chunk) -> Option<&'a mut ChunkStorage<T>>
{
}

impl ChunkStorage<voxel::Light> {
    pub fn set_type(&mut self, voxel: Voxel, ty: voxel::LightTy, intensity: u8) {
        let mut light = self.get(voxel);
        light.set(ty, intensity);
        self.set(voxel, light);
    }
}

#[cfg(test)]
mod tests {
    use crate::voxel::Kind;

    use super::*;

    #[test]
    fn get_set() {
        // Arrange
        let mut storage = ChunkStorage::<Kind>::default();

        // Act
        chunk::voxels().enumerate().for_each(|(i, v)| {
            //
            storage.set(v, (i as u16).into());
        });

        // Assert
        chunk::voxels().enumerate().for_each(|(i, v)| {
            assert_eq!(
                storage.get(v),
                (i as u16).into(),
                "Voxel {v} should have value {i}"
            );
        });
    }
}
