use std::ops::{Index, IndexMut};

use bevy::{prelude::*, utils::HashMap};

use super::{chunk, voxel};

pub type Chunk = [voxel::DataType; chunk::BUFFER_SIZE];

#[derive(Default)]
pub struct World {
    chunks: HashMap<IVec3, Chunk>,
}

impl World {
    // pub fn exists(&self, pos: IVec3) -> bool {
    //     self.chunks.contains_key(&pos)
    // }

    pub fn add(&mut self, pos: IVec3) {
        if self
            .chunks
            .insert(pos.clone(), [0; chunk::BUFFER_SIZE])
            .is_some()
        {
            panic!("Created a duplicated chunk at {:?}", &pos);
        }
    }

    // pub fn remove(&mut self, pos: IVec3) -> Option<Chunk> {
    //     self.chunks.remove(&pos)
    // }
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
        world.add(IVec3::ONE);
    }

    #[test]
    #[should_panic]
    fn add_duplicated() {
        let mut world = World::default();
        world.add(IVec3::ONE);
        world.add(IVec3::ONE);
    }

    #[test]
    fn index() {
        let mut world = World::default();
        world.add(IVec3::ONE);

        assert_eq!(world[(1, 1, 1).into()].len(), super::chunk::BUFFER_SIZE);
    }

    #[test]
    fn index_mut() {
        let mut world = World::default();
        world.add(IVec3::ONE);

        world[(1, 1, 1).into()][0] = 1;
        assert_eq!(world[IVec3::ONE][0], 1);
    }

    #[test]
    #[should_panic]
    fn invalid_index() {
        let mut world = World::default();
        world.add((0, 1, 2).into());

        world[IVec3::ONE].len();
    }

    #[test]
    #[should_panic]
    fn invalid_index_mut() {
        let mut world = World::default();
        world.add((0, 1, 2).into());

        world[IVec3::ONE][0] = 1;
        assert_eq!(world[IVec3::ONE][0], 1);
    }
}
