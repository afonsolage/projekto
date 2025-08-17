#![allow(unused)]
use crate::{
    chunk::{
        self, Chunk,
        column::ChunkColumnStorage,
        sub_chunk::{self, ChunkPack, SubChunkStorage},
    },
    coords::ChunkVoxel,
    voxel::{self},
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub trait ChunkStorageType:
    Clone + Copy + core::fmt::Debug + Default + PartialEq + Eq + PartialOrd + std::hash::Hash
{
}

#[derive(Clone, Serialize, Deserialize)]
// pub struct ChunkStorage<T>(SubChunkStorage<T>);
pub struct ChunkStorage<T>(ChunkColumnStorage<T>);

impl<T: ChunkStorageType> std::fmt::Debug for ChunkStorage<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

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
    pub fn get(&self, voxel: ChunkVoxel) -> T {
        self.0.get(voxel)
    }

    pub fn set(&mut self, voxel: ChunkVoxel, value: T) {
        self.0.set(voxel, value);
    }

    pub fn is_default(&self) -> bool {
        self.0.is_default()
    }

    pub fn pack(&mut self) {
        self.0.pack();
    }

    pub fn all<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> bool + Copy,
    {
        self.0.all(f)
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
    pub fn set_type(&mut self, voxel: ChunkVoxel, ty: voxel::LightTy, intensity: u8) {
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
