use bevy::{prelude::*, utils::hashbrown::HashMap};
use projekto_core::{
    chunk::{self, ChunkKind, ChunkLight, ChunkStorage, ChunkStorageType},
    voxel,
};

#[derive(Default, Debug)]
pub struct ChunkWorldRes<T> {
    map: HashMap<IVec3, T>,
}

impl<T> ChunkWorldRes<T> {
    pub fn get(&self, local: IVec3) -> Option<&T> {
        self.map.get(&local)
    }

    pub fn exists(&self, local: IVec3) -> bool {
        self.map.contains_key(&local)
    }

    pub fn list_chunks(&self) -> Vec<IVec3> {
        self.map.keys().cloned().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&IVec3, &T)> {
        self.map.iter()
    }
}

impl<T> ChunkWorldRes<ChunkStorage<T>>
where
    T: ChunkStorageType,
{
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

pub type ChunkKindRes = ChunkWorldRes<ChunkKind>;
pub type ChunkLightRes = ChunkWorldRes<ChunkLight>;
pub type ChunkVertexRes = ChunkWorldRes<Vec<voxel::VoxelVertex>>;

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

impl<T> From<Vec<(IVec3, T)>> for ChunkWorldRes<T> {
    fn from(v: Vec<(IVec3, T)>) -> Self {
        Self {
            map: HashMap::from_iter(v),
        }
    }
}
