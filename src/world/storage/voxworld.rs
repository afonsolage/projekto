use bevy::{prelude::*, utils::HashMap};

use super::{
    chunk::{Chunk, ChunkNeighborhood},
    voxel::{self},
};

#[derive(Default)]
pub struct VoxWorld {
    chunks: HashMap<IVec3, Chunk>,
    //Vertices
    //Fluids
    //so on
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

    pub fn update_neighborhood(&mut self, local: IVec3) {
        let mut neighborhood = ChunkNeighborhood::default();
        for side in voxel::SIDES {
            let dir = side.dir();
            let neighbor = local + dir;

            if let Some(neighbor_chunk) = self.get(neighbor) {
                neighborhood.set(side, &neighbor_chunk.kinds);
            }
        }

        if let Some(chunk) = self.get_mut(local) {
            chunk.kinds.neighborhood = neighborhood;
        }
    }

    pub fn list_chunks(&self) -> Vec<IVec3> {
        self.chunks.keys().cloned().collect()
    }
}

#[cfg(test)]
mod test {
    use bevy::math::IVec3;

    use crate::world::storage::{
        chunk::{self},
        voxel,
    };

    use super::*;

    #[test]
    fn add() {
        let mut world = VoxWorld::default();
        assert!(world.get(IVec3::ONE).is_none());
        world.add(IVec3::ONE, Default::default());
        assert!(world.get(IVec3::ONE).is_some());
    }

    #[test]
    #[should_panic]
    fn add_duplicated() {
        let mut world = VoxWorld::default();
        world.add(IVec3::ONE, Default::default());
        world.add(IVec3::ONE, Default::default());
    }

    #[test]
    fn remove() {
        let mut world = VoxWorld::default();
        world.add(IVec3::ONE, Default::default());
        assert!(world.remove(IVec3::ONE).is_some());
        assert!(world.get(IVec3::ONE).is_none());
    }

    #[test]
    fn remove_none() {
        let mut world = VoxWorld::default();
        assert!(world.remove(IVec3::ONE).is_none());
        assert!(world.get(IVec3::ONE).is_none());
    }

    #[test]
    fn update_neighborhood() {
        let mut world = VoxWorld::default();

        let center = (1, 1, 1).into();
        let mut chunk = Chunk::default();
        chunk.kinds.set_all(10.into());
        world.add(center, chunk);

        for side in voxel::SIDES {
            let dir = side.dir();
            let pos = center + dir;
            let mut chunk = Chunk::default();
            chunk.kinds.set_all((side as u16).into());
            world.add(pos, chunk);
        }

        world.update_neighborhood(center);
        let chunk = world.get(center).unwrap();

        for side in voxel::SIDES {
            match side {
                voxel::Side::Right => {
                    for a in 0..chunk::Y_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (0, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Left => {
                    for a in 0..chunk::Y_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (chunk::X_END as i32, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Up => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, 0, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Down => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, chunk::Y_END as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Front => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Y_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, b as i32, 0).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Back => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Y_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, b as i32, chunk::Z_END as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
            }
        }
    }
}
