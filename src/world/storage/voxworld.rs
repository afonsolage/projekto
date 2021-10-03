use bevy::{prelude::*, utils::HashMap};

use super::chunk::Chunk;

#[derive(Default)]
pub struct VoxWorld {
    chunks: HashMap<IVec3, Chunk>,
}

impl VoxWorld {
    pub fn add(&mut self, local: IVec3, chunk: Chunk) {
        if self.chunks.insert(local, chunk).is_some() {
            panic!("Created a duplicated chunk at {:?}", &local);
        }
    }

    pub fn remove(&mut self, local: IVec3) -> Option<Chunk> {
        self.chunks.remove(&local)
    }

    pub fn get(&self, local: IVec3) -> Option<&Chunk> {
        self.chunks.get(&local)
    }

    pub fn get_mut(&mut self, local: IVec3) -> Option<&mut Chunk> {
        self.chunks.get_mut(&local)
    }
}

#[cfg(test)]
mod test {
    use bevy::math::IVec3;

    use crate::world::storage::chunk::Chunk;

    use super::VoxWorld;

    #[test]
    fn add() {
        let mut world = VoxWorld::default();
        assert!(world.get(IVec3::ONE).is_none());
        world.add(IVec3::ONE, Chunk::default());
        assert!(world.get(IVec3::ONE).is_some());
    }

    #[test]
    #[should_panic]
    fn add_duplicated() {
        let mut world = VoxWorld::default();
        world.add(IVec3::ONE, Chunk::default());
        world.add(IVec3::ONE, Chunk::default());
    }

    #[test]
    fn remove() {
        let mut world = VoxWorld::default();
        world.add(IVec3::ONE, Chunk::default());
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
