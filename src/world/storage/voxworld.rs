use bevy::{prelude::*, utils::HashMap};

use super::{
    chunk::{ChunkKind, ChunkNeighborhood},
    voxel,
};

#[derive(Default)]
pub struct VoxWorld {
    chunks: HashMap<IVec3, ChunkKind>,
}

impl VoxWorld {
    pub fn add(&mut self, local: IVec3, kind: ChunkKind) {
        if self.chunks.insert(local, kind).is_some() {
            panic!("Created a duplicated chunk at {:?}", &local);
        }
    }

    pub fn remove(&mut self, local: IVec3) -> Option<ChunkKind> {
        self.chunks.remove(&local)
    }

    pub fn get(&self, local: IVec3) -> Option<&ChunkKind> {
        self.chunks.get(&local)
    }

    pub fn get_mut(&mut self, local: IVec3) -> Option<&mut ChunkKind> {
        self.chunks.get_mut(&local)
    }

    pub fn update_neighborhood(&mut self, local: IVec3) {
        let mut neighborhood = ChunkNeighborhood::default();
        for side in voxel::SIDES {
            let dir = side.dir();
            let neighbor = local + dir;

            if let Some(neighbor_chunk) = self.get(neighbor) {
                neighborhood.set(side, neighbor_chunk);
            }
        }

        if let Some(chunk) = self.get_mut(local) {
            chunk.neighborhood = neighborhood;
        }
    }
}

#[cfg(test)]
mod test {
    use bevy::math::IVec3;

    use crate::world::storage::{
        chunk::{self, ChunkKind},
        voxel,
    };

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

    #[test]
    fn update_neighborhood() {
        let mut world = VoxWorld::default();

        let center = (1, 1, 1).into();
        let mut kind = ChunkKind::default();
        kind.set_all(10.into());
        world.add(center, kind);

        for side in voxel::SIDES {
            let dir = side.dir();
            let pos = center + dir;
            let mut kind = ChunkKind::default();
            kind.set_all((side as u16).into());
            world.add(pos, kind);
        }

        world.update_neighborhood(center);
        let kind = world.get(center).unwrap();

        for side in voxel::SIDES {
            match side {
                voxel::Side::Right => {
                    for a in 0..chunk::AXIS_SIZE {
                        for b in 0..chunk::AXIS_SIZE {
                            assert_eq!(
                                kind.neighborhood.get(side, (0, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Left => {
                    for a in 0..chunk::AXIS_SIZE {
                        for b in 0..chunk::AXIS_SIZE {
                            assert_eq!(
                                kind.neighborhood.get(
                                    side,
                                    (chunk::AXIS_ENDING as i32, a as i32, b as i32).into()
                                ),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Up => {
                    for a in 0..chunk::AXIS_SIZE {
                        for b in 0..chunk::AXIS_SIZE {
                            assert_eq!(
                                kind.neighborhood.get(side, (a as i32, 0, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Down => {
                    for a in 0..chunk::AXIS_SIZE {
                        for b in 0..chunk::AXIS_SIZE {
                            assert_eq!(
                                kind.neighborhood.get(
                                    side,
                                    (a as i32, chunk::AXIS_ENDING as i32, b as i32).into()
                                ),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Front => {
                    for a in 0..chunk::AXIS_SIZE {
                        for b in 0..chunk::AXIS_SIZE {
                            assert_eq!(
                                kind.neighborhood.get(side, (a as i32, b as i32, 0).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Back => {
                    for a in 0..chunk::AXIS_SIZE {
                        for b in 0..chunk::AXIS_SIZE {
                            assert_eq!(
                                kind.neighborhood.get(
                                    side,
                                    (a as i32, b as i32, chunk::AXIS_ENDING as i32).into()
                                ),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
            }
        }
    }
}
