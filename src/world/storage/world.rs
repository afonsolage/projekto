use bevy::{prelude::*, utils::HashMap};

use super::chunk::Chunk;

#[derive(Default)]
pub struct World {
    chunks: HashMap<IVec3, Chunk>,
}

impl World {
    pub fn add(&mut self, pos: IVec3) {
        if self.chunks.insert(pos.clone(), Chunk::default()).is_some() {
            panic!("Created a duplicated chunk at {:?}", &pos);
        }
    }

    pub fn remove(&mut self, pos: IVec3) -> Option<Chunk> {
        self.chunks.remove(&pos)
    }

    pub fn get(&self, pos: IVec3) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }

    pub fn get_mut(&mut self, pos: IVec3) -> Option<&mut Chunk> {
        self.chunks.get_mut(&pos)
    }
}

#[cfg(test)]
mod test {
    use bevy::math::IVec3;

    use super::World;

    #[test]
    fn add() {
        let mut world = World::default();
        assert!(world.get(IVec3::ONE).is_none());
        world.add(IVec3::ONE);
        assert!(world.get(IVec3::ONE).is_some());
    }

    #[test]
    #[should_panic]
    fn add_duplicated() {
        let mut world = World::default();
        world.add(IVec3::ONE);
        world.add(IVec3::ONE);
    }

    #[test]
    fn remove() {
        let mut world = World::default();
        world.add(IVec3::ONE);
        assert!(world.remove(IVec3::ONE).is_some());
        assert!(world.get(IVec3::ONE).is_none());
    }

    #[test]
    fn remove_none() {
        let mut world = World::default();
        assert!(world.remove(IVec3::ONE).is_none());
        assert!(world.get(IVec3::ONE).is_none());
    }
}
