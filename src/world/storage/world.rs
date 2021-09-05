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

impl Index<(i32, i32, i32)> for World {
    type Output = Chunk;

    fn index(&self, index: (i32, i32, i32)) -> &Self::Output {
        &self.chunks[&(index.into())]
    }
}

impl IndexMut<(i32, i32, i32)> for World {
    fn index_mut(&mut self, index: (i32, i32, i32)) -> &mut Self::Output {
        self.chunks
            .get_mut(&(index.into()))
            .expect(format!("No entry {:?} found on HashMap", &index).as_str())
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
