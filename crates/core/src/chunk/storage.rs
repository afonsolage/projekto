#![allow(unused)]
use crate::{
    chunk::{self, Chunk, column::ChunkColumnStorage, sub_chunk::SubChunkStorage},
    coords::ChunkVoxel,
    voxel::{self},
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub trait ChunkStorageType:
    Clone + Copy + core::fmt::Debug + Default + PartialEq + Eq + PartialOrd + std::hash::Hash
{
    /// Whether this value type is stored in sparse 16³ sections instead of 16×256×16 columns.
    const USE_SECTIONS: bool = false;
}

#[derive(Clone, Serialize, Deserialize)]
enum StorageInner<T> {
    Column(ChunkColumnStorage<T>),
    Section(SubChunkStorage<T>),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(
    try_from = "StorageInner<T>",
    bound(deserialize = "T: ChunkStorageType + serde::Deserialize<'de>")
)]
pub struct ChunkStorage<T>(StorageInner<T>);

impl<T: ChunkStorageType> TryFrom<StorageInner<T>> for ChunkStorage<T> {
    type Error = String;

    fn try_from(inner: StorageInner<T>) -> Result<Self, Self::Error> {
        match (&inner, T::USE_SECTIONS) {
            (StorageInner::Section(_), true) | (StorageInner::Column(_), false) => Ok(Self(inner)),
            _ => Err("storage backend does not match its value type".to_string()),
        }
    }
}

impl<T: ChunkStorageType> std::fmt::Debug for ChunkStorage<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            StorageInner::Column(storage) => storage.fmt(f),
            StorageInner::Section(storage) => storage.fmt(f),
        }
    }
}

impl<T: ChunkStorageType> PartialEq for ChunkStorage<T> {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (StorageInner::Column(a), StorageInner::Column(b)) => a == b,
            (StorageInner::Section(a), StorageInner::Section(b)) => a == b,
            _ => false,
        }
    }
}

impl<T: ChunkStorageType> std::default::Default for ChunkStorage<T> {
    fn default() -> Self {
        if T::USE_SECTIONS {
            Self(StorageInner::Section(Default::default()))
        } else {
            Self(StorageInner::Column(Default::default()))
        }
    }
}

impl<T: ChunkStorageType> ChunkStorage<T> {
    pub fn get(&self, voxel: ChunkVoxel) -> T {
        match &self.0 {
            StorageInner::Column(storage) => storage.get(voxel),
            StorageInner::Section(storage) => storage.get(voxel),
        }
    }

    pub fn set(&mut self, voxel: ChunkVoxel, value: T) {
        match &mut self.0 {
            StorageInner::Column(storage) => storage.set(voxel, value),
            StorageInner::Section(storage) => storage.set(voxel, value),
        }
    }

    pub fn is_default(&self) -> bool {
        match &self.0 {
            StorageInner::Column(storage) => storage.is_default(),
            StorageInner::Section(storage) => storage.is_default(),
        }
    }

    /// Sets every voxel to `value` in bulk.
    pub fn fill(&mut self, value: T) {
        match &mut self.0 {
            StorageInner::Column(storage) => storage.fill(value),
            StorageInner::Section(storage) => storage.fill(value),
        }
    }

    pub fn pack(&mut self) {
        match &mut self.0 {
            StorageInner::Column(storage) => storage.pack(),
            StorageInner::Section(storage) => storage.pack(),
        }
    }

    pub fn all<F>(&self, f: F) -> bool
    where
        F: FnMut(&T) -> bool + Copy,
    {
        match &self.0 {
            StorageInner::Column(storage) => storage.all(f),
            StorageInner::Section(storage) => storage.all(f),
        }
    }

    /// Returns the value of a uniform horizontal layer, or `None` when it is mixed or the
    /// storage backend is column-based.
    pub fn layer_uniform(&self, y: usize) -> Option<T> {
        match &self.0 {
            StorageInner::Column(_) => None,
            StorageInner::Section(storage) => storage.layer_uniform(y),
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
    pub fn set_type(&mut self, voxel: ChunkVoxel, ty: voxel::LightTy, intensity: u8) {
        let mut light = self.get(voxel);
        light.set(ty, intensity);
        self.set(voxel, light);
    }
}

#[cfg(test)]
mod tests {
    use crate::voxel::{self, Kind};

    use super::*;

    #[test]
    fn get_set() {
        // Arrange
        let mut storage = ChunkStorage::<Kind>::default();

        // Act
        chunk::voxels().enumerate().for_each(|(i, v)| {
            storage.set(v, ((i % 255) as u16).into());
        });

        // Assert
        chunk::voxels().enumerate().for_each(|(i, v)| {
            let value = (i % 255) as u16;

            assert_eq!(
                storage.get(v),
                value.into(),
                "Voxel {v} should have value {value}"
            );
        });
    }

    #[test]
    fn backend_is_selected_per_value_type() {
        assert!(matches!(
            ChunkStorage::<Kind>::default().0,
            StorageInner::Section(_)
        ));
        assert!(matches!(
            ChunkStorage::<voxel::Light>::default().0,
            StorageInner::Section(_)
        ));
        assert!(matches!(
            ChunkStorage::<voxel::FacesOcclusion>::default().0,
            StorageInner::Column(_)
        ));
        assert!(matches!(
            ChunkStorage::<voxel::FacesSoftLight>::default().0,
            StorageInner::Column(_)
        ));
        assert!(matches!(
            ChunkStorage::<u16>::default().0,
            StorageInner::Column(_)
        ));
        assert!(matches!(
            ChunkStorage::<(u16, u8)>::default().0,
            StorageInner::Column(_)
        ));
    }

    #[test]
    fn section_fill() {
        let mut storage = ChunkStorage::<Kind>::default();
        storage.fill(Kind::id(1));

        assert!(storage.all(|value| *value == Kind::id(1)));
    }

    #[test]
    fn deserialization_rejects_backend_mismatch() {
        let bytes = bincode::serde::encode_to_vec(
            ChunkStorage::<Kind>(StorageInner::Column(Default::default())),
            bincode::config::standard(),
        )
        .unwrap();
        assert!(
            bincode::serde::decode_from_slice::<ChunkStorage<Kind>, _>(
                &bytes,
                bincode::config::standard()
            )
            .is_err()
        );

        let bytes = bincode::serde::encode_to_vec(
            ChunkStorage::<voxel::FacesSoftLight>(StorageInner::Section(Default::default())),
            bincode::config::standard(),
        )
        .unwrap();
        assert!(
            bincode::serde::decode_from_slice::<ChunkStorage<voxel::FacesSoftLight>, _>(
                &bytes,
                bincode::config::standard()
            )
            .is_err()
        );
    }
}
