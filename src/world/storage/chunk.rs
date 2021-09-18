use std::marker::PhantomData;

use bevy::prelude::*;

use serde::{de::DeserializeOwned, ser::SerializeSeq, Deserialize, Serialize};

use crate::world::{math, storage::chunk};

use super::voxel;

pub const AXIS_SIZE: usize = 16;
pub const AXIS_INC_SIZE: usize = AXIS_SIZE - 1;

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

pub trait ChunkStorageType:
    Sized + Copy + Default + DeserializeOwned + Serialize + PartialEq
{
}

impl ChunkStorageType for u8 {}

#[derive(Debug, PartialEq)]
pub struct ChunkStorage<T: ChunkStorageType>([T; BUFFER_SIZE]);

impl<T: ChunkStorageType> Default for ChunkStorage<T> {
    fn default() -> Self {
        Self([T::default(); BUFFER_SIZE])
    }
}

impl<T: ChunkStorageType> ChunkStorage<T> {
    pub fn get(&self, local: IVec3) -> T {
        self.0[to_index(local)]
    }

    pub fn set(&mut self, local: IVec3, value: T) {
        self.0[to_index(local)] = value;
    }

    pub fn set_all(&mut self, value: T) {
        self.0.fill(value);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.iter().all(|k| *k == Default::default())
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
                let mut arr = [T::default(); chunk::BUFFER_SIZE];

                for index in 0..chunk::BUFFER_SIZE {
                    arr[index] = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(index, &self))?;
                }

                Ok(ChunkStorage(arr))
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

        for elem in self.0.iter() {
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
        || local.x == AXIS_SIZE as i32 - 1
        || local.y == AXIS_SIZE as i32 - 1
        || local.z == AXIS_SIZE as i32 - 1
}

pub fn get_boundary_dir(local: IVec3) -> IVec3 {
    const END: i32 = AXIS_INC_SIZE as i32;

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
    local.as_f32() * AXIS_SIZE as f32
}

pub fn to_local(world: Vec3) -> IVec3 {
    IVec3::new(
        (world.x / AXIS_SIZE as f32).floor() as i32,
        (world.y / AXIS_SIZE as f32).floor() as i32,
        (world.z / AXIS_SIZE as f32).floor() as i32,
    )
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

    use crate::world::storage::chunk::AXIS_SIZE;

    use super::ChunkStorage;

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
            assert_eq!(base.as_f32() * AXIS_SIZE as f32, super::to_world(base));
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
    fn is_empty() {
        let mut chunk = ChunkStorage::<u8>::default();

        assert!(chunk.is_empty());

        chunk.set((1, 1, 1).into(), 1);

        assert!(!chunk.is_empty());
    }
}
