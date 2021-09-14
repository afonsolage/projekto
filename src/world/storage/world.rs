use std::sync::{Arc, Mutex};

use bevy::{prelude::*, utils::HashMap};

use crate::debug::PerfCounterMap;

use super::chunk::ChunkKind;

#[derive(Default)]
pub struct VoxWorld {
    chunks: HashMap<IVec3, ChunkKind>,
    pub perf: Arc<Mutex<PerfCounterMap>>,
}

impl VoxWorld {
    pub fn add(&mut self, pos: IVec3) {
        if self
            .chunks
            .insert(pos.clone(), ChunkKind::default())
            .is_some()
        {
            panic!("Created a duplicated chunk at {:?}", &pos);
        }
    }

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

    use super::VoxWorld;

    #[test]
    fn add() {
        let mut world = VoxWorld::default();
        assert!(world.get(IVec3::ONE).is_none());
        world.add(IVec3::ONE);
        assert!(world.get(IVec3::ONE).is_some());
    }

    #[test]
    #[should_panic]
    fn add_duplicated() {
        let mut world = VoxWorld::default();
        world.add(IVec3::ONE);
        world.add(IVec3::ONE);
    }

    #[test]
    fn remove() {
        let mut world = VoxWorld::default();
        world.add(IVec3::ONE);
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
