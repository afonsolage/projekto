use bevy::{prelude::*, utils::hashbrown::HashMap};
use projekto_core::{
    chunk::{self, ChunkKind, ChunkLight, ChunkStorage, ChunkStorageType},
    voxel,
};

/// Read-only resource for each chunk.
/// 
/// All instances of this struct always has the same amount of items which is the number of existing chunks.
/// Each chunk is indexed by it's unique local position.
/// 
/// This resource is automatically updated at the beginning of each frame, on [`super::GenesisLabel::Collect`].
/// 
#[derive(Default, Debug)]
pub struct ChunkWorldRes<T> {
    map: HashMap<IVec3, T>,
}

impl<T> ChunkWorldRes<T> {
    /// Gets a resource of the given chunk local
    pub fn get(&self, local: IVec3) -> Option<&T> {
        self.map.get(&local)
    }

    /// Checks if a given chunk exists
    pub fn exists(&self, local: IVec3) -> bool {
        self.map.contains_key(&local)
    }

    /// List all existing chunks local
    pub fn list_chunks(&self) -> Vec<IVec3> {
        self.map.keys().cloned().collect()
    }

    /// Returns a pair iterator of chunk local and chunk resource
    pub fn iter(&self) -> impl Iterator<Item = (&IVec3, &T)> {
        self.map.iter()
    }
}

impl<T> ChunkWorldRes<ChunkStorage<T>>
where
    T: ChunkStorageType,
{
    /// Get a voxel resource at given world coordinates.
    /// Those coordinates doesn't need to be normalized.
    pub fn get_at_world(&self, world: Vec3) -> Option<T> {
        let local = chunk::to_local(world);
        let voxel = voxel::to_local(world);

        if chunk::is_within_bounds(voxel) {
            self.map.get(&local).map(|storage| storage.get(voxel))
        } else {
            None
        }
    }
}

/// [`ChunkWorldRes`] holding [`ChunkKind`]
pub type ChunkKindRes = ChunkWorldRes<ChunkKind>;

/// [`ChunkWorldRes`] holding [`ChunkLight`]
pub type ChunkLightRes = ChunkWorldRes<ChunkLight>;

/// [`ChunkWorldRes`] holding a vector of [`voxel::VoxelVertex`]
pub type ChunkVertexRes = ChunkWorldRes<Vec<voxel::VoxelVertex>>;

/// Those are implements which should be used only by genesis module
pub(super) mod impls {
    use std::ops::{Deref, DerefMut};

    use bevy::{prelude::IVec3, utils::HashMap};

    impl<T> Deref for super::ChunkWorldRes<T> {
        type Target = HashMap<IVec3, T>;

        fn deref(&self) -> &Self::Target {
            &self.map
        }
    }

    impl<T> DerefMut for super::ChunkWorldRes<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.map
        }
    }
}