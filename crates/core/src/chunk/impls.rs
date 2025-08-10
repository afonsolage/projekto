use crate::{chunk::ChunkStorageType, voxel};

impl ChunkStorageType for u8 {}
impl ChunkStorageType for u16 {}
impl ChunkStorageType for voxel::Kind {}
impl ChunkStorageType for voxel::Light {}
impl ChunkStorageType for voxel::FacesOcclusion {}
impl ChunkStorageType for voxel::FacesSoftLight {}

impl<T, O> ChunkStorageType for (T, O)
where
    T: ChunkStorageType,
    O: ChunkStorageType,
{
}

impl<T, O1, O2> ChunkStorageType for (T, O1, O2)
where
    T: ChunkStorageType,
    O1: ChunkStorageType,
    O2: ChunkStorageType,
{
}
