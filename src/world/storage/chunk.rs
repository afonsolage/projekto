use std::marker::PhantomData;

use bevy::prelude::*;

use serde::{de::DeserializeOwned, ser::SerializeSeq, Deserialize, Serialize};

use crate::world::{math, query, storage::chunk};

use super::voxel;

pub const AXIS_SIZE: usize = 16;
pub const AXIS_ENDING: usize = AXIS_SIZE - 1;

// const CHUNK_AXIS_OFFSET: usize = CHUNK_AXIS_SIZE / 2;
const BUFFER_SIZE: usize = AXIS_SIZE * AXIS_SIZE * AXIS_SIZE;

const X_MASK: usize = 0b_1111_0000_0000;
const Z_MASK: usize = 0b_0000_1111_0000;
const Y_MASK: usize = 0b_0000_0000_1111;

const X_SHIFT: usize = 8;
const Z_SHIFT: usize = 4;
const Y_SHIFT: usize = 0;

#[derive(Default)]
pub struct ChunkIter {
    iter_index: usize,
}

impl Iterator for ChunkIter {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        if self.iter_index >= BUFFER_SIZE {
            None
        } else {
            let xyz = from_index(self.iter_index);
            self.iter_index += 1;
            Some(xyz)
        }
    }
}

pub trait ChunkStorageType: Copy + Default + DeserializeOwned + Serialize + PartialEq {}

impl ChunkStorageType for u8 {}

#[derive(Debug, Clone)]
pub struct ChunkStorage<T: ChunkStorageType> {
    main: Vec<T>,
    pub neighborhood: ChunkNeighborhood<T>,
}

impl<T: ChunkStorageType> Default for ChunkStorage<T> {
    fn default() -> Self {
        Self {
            main: vec![T::default(); BUFFER_SIZE],
            neighborhood: ChunkNeighborhood::default(),
        }
    }
}

#[cfg(test)]
impl<T: ChunkStorageType> PartialEq for ChunkStorage<T> {
    fn eq(&self, other: &Self) -> bool {
        self.main == other.main
    }
}

impl<T: ChunkStorageType> ChunkStorage<T> {
    pub fn get(&self, local: IVec3) -> T {
        // if self.main.is_empty() {
        //     T::default()
        // } else {

        // }
        self.main[to_index(local)]
    }

    pub fn set(&mut self, local: IVec3, value: T) {
        // if self.main.is_empty() {
        //     self.main = vec![T::default(); BUFFER_SIZE];
        // }

        self.main[to_index(local)] = value;
    }

    #[cfg(test)]
    pub fn set_all(&mut self, value: T) {
        self.main.fill(value);
    }

    #[cfg(test)]
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.main.iter()
    }

    #[cfg(test)]
    pub fn is_default(&self) -> bool {
        self.iter().all(|&t| t == T::default())
    }
}

impl<'de, T: ChunkStorageType> Deserialize<'de> for ChunkStorage<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ArrayVisitor<T>(PhantomData<T>);
        impl<'de, T: ChunkStorageType> serde::de::Visitor<'de> for ArrayVisitor<T> {
            type Value = ChunkStorage<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_fmt(format_args!("an array of length {}", chunk::BUFFER_SIZE))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut vec = vec![];

                while let Some(element) = seq.next_element()? {
                    vec.push(element);
                }

                if vec.len() != 0 && vec.len() != chunk::BUFFER_SIZE {
                    return Err(serde::de::Error::invalid_length(vec.len(), &self));
                }

                // if vec.is_empty() {
                //     vec.shrink_to(0);
                // }

                Ok(ChunkStorage {
                    main: vec,
                    neighborhood: ChunkNeighborhood::default(),
                })
            }
        }

        deserializer.deserialize_seq(ArrayVisitor(PhantomData))
    }
}

impl<T: ChunkStorageType> Serialize for ChunkStorage<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(chunk::BUFFER_SIZE))?;

        for elem in self.main.iter() {
            seq.serialize_element(elem)?;
        }

        seq.end()
    }
}

pub type ChunkKind = ChunkStorage<voxel::Kind>;

fn to_index(local: IVec3) -> usize {
    (local.x << X_SHIFT | local.y << Y_SHIFT | local.z << Z_SHIFT) as usize
}

fn from_index(index: usize) -> IVec3 {
    IVec3::new(
        ((index & X_MASK) >> X_SHIFT) as i32,
        ((index & Y_MASK) >> Y_SHIFT) as i32,
        ((index & Z_MASK) >> Z_SHIFT) as i32,
    )
}

pub fn voxels() -> impl Iterator<Item = IVec3> {
    ChunkIter::default()
}

pub fn is_within_bounds(local: IVec3) -> bool {
    math::is_within_cubic_bounds(local, 0, AXIS_SIZE as i32 - 1)
}

pub fn is_at_bounds(local: IVec3) -> bool {
    local.x == 0
        || local.y == 0
        || local.z == 0
        || local.x == AXIS_ENDING as i32
        || local.y == AXIS_ENDING as i32
        || local.z == AXIS_ENDING as i32
}

pub fn get_boundary_dir(local: IVec3) -> IVec3 {
    const END: i32 = AXIS_ENDING as i32;

    (
        match local.x {
            0 => -1,
            END => 1,
            _ => 0,
        },
        match local.y {
            0 => -1,
            END => 1,
            _ => 0,
        },
        match local.z {
            0 => -1,
            END => 1,
            _ => 0,
        },
    )
        .into()
}

pub fn to_world(local: IVec3) -> Vec3 {
    local.as_vec3() * AXIS_SIZE as f32
}

pub fn to_local(world: Vec3) -> IVec3 {
    IVec3::new(
        (world.x / AXIS_SIZE as f32).floor() as i32,
        (world.y / AXIS_SIZE as f32).floor() as i32,
        (world.z / AXIS_SIZE as f32).floor() as i32,
    )
}

#[derive(Debug, Default, Clone)]
pub struct ChunkNeighborhood<T: ChunkStorageType>(
    [Option<[T; AXIS_SIZE * AXIS_SIZE]>; voxel::SIDE_COUNT],
);

impl<T: ChunkStorageType> ChunkNeighborhood<T> {
    pub fn set(&mut self, side: voxel::Side, chunk: &ChunkStorage<T>) {
        const END: i32 = AXIS_ENDING as i32;

        let (begin, end_inclusive) = match side {
            voxel::Side::Right => ((0, 0, 0).into(), (0, END, END).into()),
            voxel::Side::Left => ((END, 0, 0).into(), (END, END, END).into()),
            voxel::Side::Up => ((0, 0, 0).into(), (END, 0, END).into()),
            voxel::Side::Down => ((0, END, 0).into(), (END, END, END).into()),
            voxel::Side::Front => ((0, 0, 0).into(), (END, END, 0).into()),
            voxel::Side::Back => ((0, 0, END).into(), (END, END, END).into()),
        };

        let mut neighborhood_side = [T::default(); AXIS_SIZE * AXIS_SIZE];
        for pos in query::range_inclusive(begin, end_inclusive) {
            let index = Self::to_index(side, pos);
            neighborhood_side[index] = chunk.get(pos)
        }

        self.0[side as usize] = Some(neighborhood_side);
    }

    pub fn get(&self, side: voxel::Side, pos: IVec3) -> Option<T> {
        self.0[side as usize].map(|ref neighborhood_side| {
            let index = Self::to_index(side, pos);
            neighborhood_side[index]
        })
    }

    fn to_index(side: voxel::Side, pos: IVec3) -> usize {
        use voxel::Side;

        assert!(match &side {
            Side::Right => pos.x == 0,
            Side::Left => pos.x == AXIS_ENDING as i32,
            Side::Up => pos.y == 0,
            Side::Down => pos.y == AXIS_ENDING as i32,
            Side::Front => pos.z == 0,
            Side::Back => pos.z == AXIS_ENDING as i32,
        });

        let index = match side {
            Side::Right | Side::Left => (pos.z << Z_SHIFT | pos.y << Y_SHIFT) as usize,
            Side::Up | Side::Down => (pos.x << Z_SHIFT | pos.z << Y_SHIFT) as usize,
            Side::Front | Side::Back => (pos.x << Z_SHIFT | pos.y << Y_SHIFT) as usize,
        };
        index
    }
}

pub fn overlap_voxel(pos: IVec3) -> (IVec3, IVec3) {
    let overlapping_voxel = math::euclid_rem(pos, AXIS_SIZE as i32);
    let overlapping_dir = (
        if pos.x < 0 {
            -1
        } else if pos.x >= AXIS_SIZE as i32 {
            1
        } else {
            0
        },
        if pos.y < 0 {
            -1
        } else if pos.y >= AXIS_SIZE as i32 {
            1
        } else {
            0
        },
        if pos.z < 0 {
            -1
        } else if pos.z >= AXIS_SIZE as i32 {
            1
        } else {
            0
        },
    )
        .into();

    (overlapping_dir, overlapping_voxel)
}

#[cfg(test)]
mod tests {
    use bevy::math::IVec3;
    use rand::{random, Rng};

    use crate::world::storage::{
        chunk::{ChunkKind, ChunkStorageType, AXIS_ENDING, AXIS_SIZE},
        voxel,
    };

    use super::{ChunkNeighborhood, ChunkStorage};

    #[test]
    fn to_xyz() {
        assert_eq!(IVec3::new(0, 0, 0), super::from_index(0));
        assert_eq!(IVec3::new(0, 1, 0), super::from_index(1));
        assert_eq!(IVec3::new(0, 2, 0), super::from_index(2));

        assert_eq!(IVec3::new(0, 0, 1), super::from_index(super::AXIS_SIZE));
        assert_eq!(IVec3::new(0, 1, 1), super::from_index(super::AXIS_SIZE + 1));
        assert_eq!(IVec3::new(0, 2, 1), super::from_index(super::AXIS_SIZE + 2));

        assert_eq!(
            IVec3::new(1, 0, 0),
            super::from_index(super::AXIS_SIZE * super::AXIS_SIZE)
        );
        assert_eq!(
            IVec3::new(1, 1, 0),
            super::from_index(super::AXIS_SIZE * super::AXIS_SIZE + 1)
        );
        assert_eq!(
            IVec3::new(1, 2, 0),
            super::from_index(super::AXIS_SIZE * super::AXIS_SIZE + 2)
        );

        assert_eq!(
            IVec3::new(1, 0, 1),
            super::from_index(super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE)
        );
        assert_eq!(
            IVec3::new(1, 1, 1),
            super::from_index(super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE + 1)
        );
        assert_eq!(
            IVec3::new(1, 2, 1),
            super::from_index(super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE + 2)
        );
    }

    #[test]
    fn to_index() {
        assert_eq!(super::to_index((0, 0, 0).into()), 0);
        assert_eq!(super::to_index((0, 1, 0).into()), 1);
        assert_eq!(super::to_index((0, 2, 0).into()), 2);

        assert_eq!(super::to_index((0, 0, 1).into()), super::AXIS_SIZE);
        assert_eq!(super::to_index((0, 1, 1).into()), super::AXIS_SIZE + 1);
        assert_eq!(super::to_index((0, 2, 1).into()), super::AXIS_SIZE + 2);

        assert_eq!(
            super::to_index((1, 0, 0).into()),
            super::AXIS_SIZE * super::AXIS_SIZE
        );
        assert_eq!(
            super::to_index((1, 1, 0).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + 1
        );
        assert_eq!(
            super::to_index((1, 2, 0).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + 2
        );

        assert_eq!(
            super::to_index((1, 0, 1).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE
        );
        assert_eq!(
            super::to_index((1, 1, 1).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE + 1
        );
        assert_eq!(
            super::to_index((1, 2, 1).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE + 2
        );
    }

    #[test]
    fn to_world() {
        use super::*;

        const TEST_COUNT: usize = 1000;
        const MAG: f32 = 100.0;

        for _ in 0..TEST_COUNT {
            let base = IVec3::new(
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
            );

            // To world just convert from local chunk coordinates (1, 2, -1) to world coordinates (16, 32, -16)
            // assuming AXIS_SIZE = 16
            assert_eq!(base.as_vec3() * AXIS_SIZE as f32, super::to_world(base));
        }
    }

    #[test]
    fn to_local() {
        use super::*;

        assert_eq!(
            IVec3::new(0, -1, -2),
            super::to_local(Vec3::new(3.0, -0.8, -17.0))
        );
        assert_eq!(
            IVec3::new(0, -1, 0),
            super::to_local(Vec3::new(3.0, -15.8, 0.0))
        );
        assert_eq!(
            IVec3::new(-3, 1, 5),
            super::to_local(Vec3::new(-32.1, 20.0, 88.1))
        );

        const TEST_COUNT: usize = 1000;
        const MAG: f32 = 100.0;

        for _ in 0..TEST_COUNT {
            let base = IVec3::new(
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
            );

            // This fragment is just used to check if rounding will be correct, since it should not affect
            // the overall chunk local position
            let frag = Vec3::new(
                random::<f32>() * (AXIS_SIZE - 1) as f32,
                random::<f32>() * (AXIS_SIZE - 1) as f32,
                random::<f32>() * (AXIS_SIZE - 1) as f32,
            );

            let world = Vec3::new(
                (base.x * AXIS_SIZE as i32) as f32 + frag.x,
                (base.y * AXIS_SIZE as i32) as f32 + frag.y,
                (base.z * AXIS_SIZE as i32) as f32 + frag.z,
            );

            // To local convert from world chunk coordinates (15.4, 1.1, -0.5) to local coordinates (1, 0, -1)
            // assuming AXIS_SIZE = 16
            assert_eq!(base, super::to_local(world));
        }
    }

    #[test]
    fn into_iter() {
        let mut first = None;
        let mut last = IVec3::ZERO;

        for pos in super::voxels() {
            assert!(pos.x >= 0 && pos.x < super::AXIS_SIZE as i32);
            assert!(pos.y >= 0 && pos.y < super::AXIS_SIZE as i32);
            assert!(pos.z >= 0 && pos.z < super::AXIS_SIZE as i32);

            if first == None {
                first = Some(pos);
            }
            last = pos;
        }

        assert_eq!(first, Some(IVec3::ZERO));
        assert_eq!(
            last,
            (
                AXIS_SIZE as i32 - 1,
                AXIS_SIZE as i32 - 1,
                AXIS_SIZE as i32 - 1
            )
                .into()
        );
    }

    #[test]
    fn set_get() {
        let mut chunk = ChunkStorage::<u8>::default();

        let mut rnd = rand::thread_rng();
        for v in super::voxels() {
            let k = rnd.gen::<u8>();
            chunk.set(v, k);
            assert_eq!(k, chunk.get(v));
        }
    }

    #[test]
    fn overlap_voxel() {
        assert_eq!(
            super::overlap_voxel((-1, 10, 5).into()),
            ((-1, 0, 0).into(), (15, 10, 5).into())
        );
        assert_eq!(
            super::overlap_voxel((-1, 10, 16).into()),
            ((-1, 0, 1).into(), (15, 10, 0).into())
        );
        assert_eq!(
            super::overlap_voxel((0, 0, 0).into()),
            ((0, 0, 0).into(), (0, 0, 0).into())
        );
        assert_eq!(
            super::overlap_voxel((17, 10, 5).into()),
            ((1, 0, 0).into(), (1, 10, 5).into())
        );
    }

    #[test]
    fn is_default() {
        impl ChunkStorageType for [u8; 3] {}

        let mut chunk = ChunkStorage::<[u8; 3]>::default();
        assert!(chunk.is_default());

        chunk.set((1, 1, 1).into(), [1; 3]);

        assert!(!chunk.is_default());
    }

    #[test]
    fn neighborhood() {
        use super::voxel::Side;

        let mut neighborhood = ChunkNeighborhood::default();

        for side in voxel::SIDES {
            assert!(neighborhood.get(side, (0, 0, 0).into()).is_none());
        }

        let mut top = ChunkKind::default();
        top.set_all(1.into());

        neighborhood.set(voxel::Side::Up, &top);

        assert_eq!(
            neighborhood.get(voxel::Side::Up, (1, 0, 3).into()),
            Some(1.into())
        );

        let mut kinds_set = vec![];
        let mut chunks = vec![ChunkKind::default(); voxel::SIDE_COUNT];

        for side in voxel::SIDES {
            for _ in 0..1000 {
                let mut rnd = rand::thread_rng();
                let kind = rnd.gen_range(1..10).into();
                let mut pos: IVec3 = (
                    rnd.gen_range(0..AXIS_SIZE) as i32,
                    rnd.gen_range(0..AXIS_SIZE) as i32,
                    rnd.gen_range(0..AXIS_SIZE) as i32,
                )
                    .into();

                match side {
                    Side::Right => pos.x = 0,
                    Side::Left => pos.x = AXIS_ENDING as i32,
                    Side::Up => pos.y = 0,
                    Side::Down => pos.y = AXIS_ENDING as i32,
                    Side::Front => pos.z = 0,
                    Side::Back => pos.z = AXIS_ENDING as i32,
                }

                // Avoid setting different values on same voxel
                if chunks[side as usize].get(pos) == voxel::Kind::default() {
                    kinds_set.push((side, pos, kind));
                    chunks[side as usize].set(pos, kind);
                }
            }
        }

        for side in voxel::SIDES {
            if !chunks[side as usize].is_default() {
                neighborhood.set(side, &chunks[side as usize]);
            }
        }

        for (side, pos, kind) in kinds_set {
            assert_eq!(
                neighborhood.get(side, pos),
                Some(kind),
                "neighborhood get {:?} {} != {:?}",
                side,
                pos,
                Some(kind)
            );
        }
    }

    #[test]
    fn is_at_bounds() {
        let local = (1, 1, 1).into();
        assert_eq!(super::is_at_bounds(local), false);

        let local = (1, 0, 1).into();
        assert_eq!(super::is_at_bounds(local), true);

        let local = (1, AXIS_ENDING as i32, 1).into();
        assert_eq!(super::is_at_bounds(local), true);

        let local = (0, 0, 0).into();
        assert_eq!(super::is_at_bounds(local), true);

        let local = (2, 1, 14).into();
        assert_eq!(super::is_at_bounds(local), false);
    }

    #[test]
    fn get_boundary_dir() {
        let local = (0, 0, 0).into();
        assert_eq!(super::get_boundary_dir(local), (-1, -1, -1).into());

        let local = (1, 2, 3).into();
        assert_eq!(super::get_boundary_dir(local), (0, 0, 0).into());

        let local = (AXIS_ENDING as i32, 2, 3).into();
        assert_eq!(super::get_boundary_dir(local), (1, 0, 0).into());

        let local = (AXIS_ENDING as i32, AXIS_ENDING as i32, AXIS_ENDING as i32).into();
        assert_eq!(super::get_boundary_dir(local), (1, 1, 1).into());
    }
}
