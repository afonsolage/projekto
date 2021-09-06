use std::ops::{Index, IndexMut};

use bevy::{prelude::*, utils::HashMap};

use super::chunk::Chunk;

#[derive(Default)]
pub struct World {
    chunks: HashMap<IVec3, Chunk>,
}

impl World {
    pub fn exists(&self, pos: IVec3) -> bool {
        self.chunks.contains_key(&pos)
    }

    pub fn add(&mut self, pos: IVec3) {
        if self.chunks.insert(pos.clone(), Chunk::default()).is_some() {
            panic!("Created a duplicated chunk at {:?}", &pos);
        }
    }

    pub fn remove(&mut self, pos: IVec3) -> Option<Chunk> {
        self.chunks.remove(&pos)
    }
}

impl Index<IVec3> for World {
    type Output = Chunk;

    fn index(&self, index: IVec3) -> &Self::Output {
        &self.chunks[&index]
    }
}

impl IndexMut<IVec3> for World {
    fn index_mut(&mut self, index: IVec3) -> &mut Self::Output {
        self.chunks
            .get_mut(&index)
            .expect(format!("No entry {} found on HashMap", &index).as_str())
    }
}

#[cfg(test)]
mod test {
    use bevy::math::IVec3;

    use super::World;

    #[test]
    fn add() {
        let mut world = World::default();
        assert!(!world.exists(IVec3::ONE));
        world.add(IVec3::ONE);
        assert!(world.exists(IVec3::ONE));
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
        assert!(!world.exists(IVec3::ONE));
    }

    #[test]
    fn remove_none() {
        let mut world = World::default();
        assert!(world.remove(IVec3::ONE).is_none());
        assert!(!world.exists(IVec3::ONE));
    }

    #[test]
    fn exists() {
        let mut world = World::default();
        assert!(!world.exists(IVec3::ONE));
        world.add(IVec3::ONE);
        assert!(world.exists(IVec3::ONE));
    }

    #[test]
    fn index() {
        let mut world = World::default();
        world.add(IVec3::ONE);

        assert!(world.exists((1, 1, 1).into()));
    }

    #[test]
    fn index_mut() {
        let mut world = World::default();
        world.add(IVec3::ONE);

        world[(1, 1, 1).into()].set_voxel_kind(IVec3::ZERO, 1);
        assert_eq!(world[IVec3::ONE].get_voxel_kind(IVec3::ZERO), 1);
    }

    #[test]
    #[should_panic]
    fn invalid_index() {
        let mut world = World::default();
        world.add((0, 1, 2).into());

        world[IVec3::ONE].get_voxel_kind(IVec3::ZERO);
    }

    #[test]
    #[should_panic]
    fn invalid_index_mut() {
        let mut world = World::default();
        world.add((0, 1, 2).into());

        world[IVec3::ONE].set_voxel_kind(IVec3::ZERO, 1);
    }
}
