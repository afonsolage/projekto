use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    chunk::{self, Chunk, BUFFER_SIZE},
    voxel::{self, Voxel},
};

pub trait ChunkStorageType:
    Clone + Copy + core::fmt::Debug + Default + PartialEq + Eq + PartialOrd + std::hash::Hash
{
}

impl ChunkStorageType for u8 {}
impl ChunkStorageType for voxel::Kind {}
impl ChunkStorageType for voxel::Light {}
impl ChunkStorageType for voxel::FacesOcclusion {}
impl ChunkStorageType for voxel::FacesSoftLight {}

#[derive(Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
enum StorageValue<T> {
    Single(T),
    Dense(Vec<T>),
}

impl<T: ChunkStorageType> StorageValue<T> {
    fn as_multiple(&mut self) {
        match self {
            StorageValue::Single(t) => {
                let t = *t;
                let _ = std::mem::replace(self, StorageValue::Dense(vec![t; BUFFER_SIZE]));
            }
            StorageValue::Dense(_) => (),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChunkStorage<T>(StorageValue<T>);

impl<T: ChunkStorageType> PartialEq for ChunkStorage<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: ChunkStorageType> std::fmt::Debug for ChunkStorage<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = match &self.0 {
            StorageValue::Single(_) => 0,
            StorageValue::Dense(buf) => buf.len(),
        };
        write!(f, "ChunkStorage(len: {len})")
    }
}

impl<T: ChunkStorageType> std::default::Default for ChunkStorage<T> {
    fn default() -> Self {
        Self(StorageValue::Single(T::default()))
    }
}

impl<T: ChunkStorageType> ChunkStorage<T> {
    pub fn get(&self, voxel: Voxel) -> T {
        match &self.0 {
            StorageValue::Dense(b) => b[chunk::to_index(voxel)],
            StorageValue::Single(t) => *t,
        }
    }

    pub fn set(&mut self, voxel: Voxel, value: T) {
        match &mut self.0 {
            StorageValue::Single(v) if *v != value => {
                self.0.as_multiple();
                self.set(voxel, value);
            }
            StorageValue::Dense(items) => items[chunk::to_index(voxel)] = value,
            _ => (),
        }
    }

    pub fn is_default(&self) -> bool {
        matches!(self.0, StorageValue::Single(_))
    }

    pub fn all<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> bool,
    {
        match &self.0 {
            StorageValue::Dense(buf) => buf.iter().all(f),
            StorageValue::Single(t) => f(t),
        }
    }
}
impl<T: ChunkStorageType> std::ops::Index<usize> for ChunkStorage<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < BUFFER_SIZE);
        match &self.0 {
            StorageValue::Dense(buf) => &buf[index],
            StorageValue::Single(t) => t,
        }
    }
}

impl<T: ChunkStorageType> std::ops::IndexMut<usize> for ChunkStorage<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < BUFFER_SIZE);

        // Had to do this to avoid the borrow checker yelling at me
        if matches!(self.0, StorageValue::Dense(_)) {
            if let StorageValue::Dense(items) = &mut self.0 {
                &mut items[index]
            } else {
                unreachable!()
            }
        } else {
            self.0.as_multiple();
            self.index_mut(index)
        }
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
