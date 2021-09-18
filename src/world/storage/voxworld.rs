use bevy::{prelude::*, utils::HashMap};

use super::chunk::ChunkKind;

#[derive(Default)]
pub struct VoxWorld {
    chunks: HashMap<IVec3, ChunkKind>,
}

impl VoxWorld {
    pub fn add(&mut self, pos: IVec3, kind: ChunkKind) {
        if self.chunks.insert(pos.clone(), kind).is_some() {
            panic!("Created a duplicated chunk at {:?}", &pos);
        }
    }

    #[cfg(test)]
    pub fn remove(&mut self, pos: IVec3) -> Option<ChunkKind> {
        self.chunks.remove(&pos)
    }

    pub fn get(&self, pos: IVec3) -> Option<&ChunkKind> {
        self.chunks.get(&pos)
    }

    pub fn get_mut(&mut self, pos: IVec3) -> Option<&mut ChunkKind> {
        self.chunks.get_mut(&pos)
    }
}

#[cfg(test)]
mod test {
    use bevy::math::IVec3;

    use crate::world::storage::chunk::ChunkKind;

    use super::VoxWorld;

    #[test]
    fn add() {
        let mut world = VoxWorld::default();
        assert!(world.get(IVec3::ONE).is_none());
        world.add(IVec3::ONE, ChunkKind::default());
        assert!(world.get(IVec3::ONE).is_some());
    }

    #[test]
    #[should_panic]
    fn add_duplicated() {
        let mut world = VoxWorld::default();
        world.add(IVec3::ONE, ChunkKind::default());
        world.add(IVec3::ONE, ChunkKind::default());
    }

    #[test]
    fn remove() {
        let mut world = VoxWorld::default();
        world.add(IVec3::ONE, ChunkKind::default());
        assert!(world.remove(IVec3::ONE).is_some());
        assert!(world.get(IVec3::ONE).is_none());
    }

    #[test]
    fn remove_none() {
        let mut world = VoxWorld::default();
        assert!(world.remove(IVec3::ONE).is_none());
        assert!(world.get(IVec3::ONE).is_none());
    }
}
