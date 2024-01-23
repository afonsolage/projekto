use bevy_math::{IVec2, IVec3, Vec3};
// de::DeserializeOwned,
use serde::{Deserialize, Serialize};

use crate::{math, query};

use super::voxel;

pub const X_AXIS_SIZE: usize = 16;
pub const Y_AXIS_SIZE: usize = 256;
pub const Z_AXIS_SIZE: usize = 16;

pub const X_END: i32 = (X_AXIS_SIZE - 1) as i32;
pub const Y_END: i32 = (Y_AXIS_SIZE - 1) as i32;
pub const Z_END: i32 = (Z_AXIS_SIZE - 1) as i32;

pub const BUFFER_SIZE: usize = X_AXIS_SIZE * Z_AXIS_SIZE * Y_AXIS_SIZE;

const X_SHIFT: usize = (Z_AXIS_SIZE.ilog2() + Z_SHIFT as u32) as usize;
const Z_SHIFT: usize = Y_AXIS_SIZE.ilog2() as usize;
const Y_SHIFT: usize = 0;

const X_MASK: usize = (X_AXIS_SIZE - 1) << X_SHIFT;
const Z_MASK: usize = (Z_AXIS_SIZE - 1) << Z_SHIFT;
const Y_MASK: usize = Y_AXIS_SIZE - 1;

#[cfg(feature = "mem_alloc")]
pub static ALLOC_COUNT: once_cell::sync::Lazy<std::sync::atomic::AtomicUsize> =
    once_cell::sync::Lazy::new(std::sync::atomic::AtomicUsize::default);

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Chunk {
    pub kinds: ChunkKind,
    pub lights: ChunkLight,
    pub vertices: Vec<voxel::VoxelVertex>,
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.kinds == other.kinds && self.vertices == other.vertices
    }
}

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
    Clone + Copy + core::fmt::Debug + Default + PartialEq + PartialOrd
{
}

impl ChunkStorageType for u8 {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkStorage<T> {
    main: Vec<T>,
    pub neighborhood: ChunkNeighborhood<T>,
}

impl<T: ChunkStorageType> Default for ChunkStorage<T> {
    fn default() -> Self {
        Self::new(vec![T::default(); BUFFER_SIZE])
    }
}

impl<T: ChunkStorageType> Clone for ChunkStorage<T> {
    fn clone(&self) -> Self {
        let mut cloned = Self::new(self.main.clone());
        cloned.neighborhood = self.neighborhood.clone();
        cloned
    }
}

impl<T: ChunkStorageType> PartialEq for ChunkStorage<T> {
    fn eq(&self, other: &Self) -> bool {
        self.main == other.main
    }
}

impl<T: ChunkStorageType> ChunkStorage<T> {
    fn new(main: Vec<T>) -> Self {
        #[cfg(feature = "mem_alloc")]
        ALLOC_COUNT.fetch_add(1, std::sync::atomic::Ordering::AcqRel);

        Self {
            main,
            neighborhood: ChunkNeighborhood::default(),
        }
    }

    #[inline]
    pub fn get(&self, local: IVec3) -> T {
        self.main[to_index(local)]
    }

    pub fn set(&mut self, local: IVec3, value: T) {
        self.main[to_index(local)] = value;
    }

    #[inline]
    pub fn get_absolute(&self, local: IVec3) -> Option<T> {
        if !is_within_bounds(local) {
            let (dir, next_chunk_voxel) = overlap_voxel(local);

            let side = voxel::Side::from_dir(dir);

            self.neighborhood.get(side, next_chunk_voxel)
        } else {
            Some(self.get(local))
        }
    }

    pub fn set_all(&mut self, value: T) {
        self.main.fill(value);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.main.iter()
    }

    pub fn is_default(&self) -> bool {
        // TODO: Add a clever way to check if ChunkStorage wasn't initialized;
        self.is_all(T::default())
    }

    pub fn is_all(&self, value: T) -> bool {
        self.iter().all(|t| *t == value)
    }

    pub fn copy_from(&mut self, other: &Self) {
        self.main.copy_from_slice(&other.main);
    }
}

impl<T: ChunkStorageType> std::ops::Index<usize> for ChunkStorage<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < BUFFER_SIZE);
        &self.main[index]
    }
}

impl<T: ChunkStorageType> std::ops::IndexMut<usize> for ChunkStorage<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < BUFFER_SIZE);
        &mut self.main[index]
    }
}

#[cfg(feature = "mem_alloc")]
impl<T: ChunkStorageType> Drop for ChunkStorage<T> {
    fn drop(&mut self) {
        ALLOC_COUNT.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

pub trait GetChunkStorage<'a, T: ChunkStorageType + 'a>:
    Fn(IVec3) -> Option<&'a ChunkStorage<T>>
{
}

impl<'a, T: ChunkStorageType + 'a> GetChunkStorage<'a, T> for T where
    T: Fn(IVec3) -> Option<&'a ChunkStorage<T>>
{
}

pub trait GetChunkStorageMut<'a, T: ChunkStorageType + 'a>:
    FnMut(IVec3) -> Option<&'a mut ChunkStorage<T>>
{
}

impl<'a, T: ChunkStorageType + 'a> GetChunkStorageMut<'a, T> for T where
    T: FnMut(IVec3) -> Option<&'a mut ChunkStorage<T>>
{
}

pub type ChunkKind = ChunkStorage<voxel::Kind>;
pub type ChunkLight = ChunkStorage<voxel::Light>;

impl ChunkLight {
    pub fn set_type(&mut self, local: IVec3, ty: voxel::LightTy, intensity: u8) {
        let mut light = self.get(local);
        light.set(ty, intensity);
        self.set(local, light);
    }
}

#[inline]
pub fn to_index(local: IVec3) -> usize {
    (local.x << X_SHIFT | local.y << Y_SHIFT | local.z << Z_SHIFT) as usize
}

#[inline]
pub fn to_index_2d(local: IVec2) -> usize {
    (local.x << Z_AXIS_SIZE.ilog2() | local.y << Y_SHIFT) as usize
}

#[inline]
pub fn from_index(index: usize) -> IVec3 {
    IVec3::new(
        ((index & X_MASK) >> X_SHIFT) as i32,
        ((index & Y_MASK) >> Y_SHIFT) as i32,
        ((index & Z_MASK) >> Z_SHIFT) as i32,
    )
}

pub fn voxels() -> impl Iterator<Item = IVec3> {
    ChunkIter::default()
}

#[inline]
pub fn is_within_bounds(local: IVec3) -> bool {
    local.x >= 0
        && local.x < X_AXIS_SIZE as i32
        && local.z >= 0
        && local.z < Z_AXIS_SIZE as i32
        && local.y >= 0
        && local.y < Y_AXIS_SIZE as i32
}

#[inline]
pub fn is_at_bounds(local: IVec3) -> bool {
    local.x == 0
        || local.y == 0
        || local.z == 0
        || local.x == (X_AXIS_SIZE - 1) as i32
        || local.y == (Y_AXIS_SIZE - 1) as i32
        || local.z == (Z_AXIS_SIZE - 1) as i32
}

#[inline]
pub fn neighboring(local: IVec3, voxel: IVec3) -> Vec<IVec3> {
    math::to_unit_dir(get_boundary_dir(voxel))
        .into_iter()
        .map(|dir| dir + local)
        .collect()
}

pub fn get_boundary_dir(local: IVec3) -> IVec3 {
    (
        match local.x {
            0 => -1,
            X_END => 1,
            _ => 0,
        },
        match local.y {
            0 => -1,
            Y_END => 1,
            _ => 0,
        },
        match local.z {
            0 => -1,
            Z_END => 1,
            _ => 0,
        },
    )
        .into()
}

pub fn to_world(local: IVec3) -> Vec3 {
    local.as_vec3() * Vec3::new(X_AXIS_SIZE as f32, Y_AXIS_SIZE as f32, Z_AXIS_SIZE as f32)
}

pub fn to_local(world: Vec3) -> IVec3 {
    IVec3::new(
        (world.x / X_AXIS_SIZE as f32).floor() as i32,
        (world.y / Y_AXIS_SIZE as f32).floor() as i32,
        (world.z / Z_AXIS_SIZE as f32).floor() as i32,
    )
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ChunkNeighborhood<T>([Option<Vec<T>>; voxel::SIDE_COUNT]);

impl<T: ChunkStorageType> ChunkNeighborhood<T> {
    fn get_side_range(side: voxel::Side) -> (IVec3, IVec3) {
        match side {
            voxel::Side::Right => ((0, 0, 0).into(), (0, Y_END, Z_END).into()),
            voxel::Side::Left => ((X_END, 0, 0).into(), (X_END, Y_END, Z_END).into()),
            voxel::Side::Up => ((0, 0, 0).into(), (X_END, 0, Z_END).into()),
            voxel::Side::Down => ((0, Y_END, 0).into(), (X_END, Y_END, Z_END).into()),
            voxel::Side::Front => ((0, 0, 0).into(), (X_END, Y_END, 0).into()),
            voxel::Side::Back => ((0, 0, Z_END).into(), (X_END, Y_END, Z_END).into()),
        }
    }

    pub fn side_iterator(side: voxel::Side) -> impl Iterator<Item = IVec3> {
        let (begin, end_inclusive) = Self::get_side_range(side);

        query::range_inclusive(begin, end_inclusive)
    }

    pub fn set(&mut self, side: voxel::Side, chunk: &ChunkStorage<T>) {
        let index_n_locals = Self::side_iterator(side)
            .map(|v| (Self::to_index(side, v), v))
            .collect::<Vec<_>>();

        let capacity = index_n_locals.iter().map(|(idx, _)| *idx).max().unwrap() + 1;

        let mut neighborhood_side = vec![T::default(); capacity];

        for (index, pos) in index_n_locals {
            neighborhood_side[index] = chunk.get(pos);
        }

        self.0[side as usize] = Some(neighborhood_side);
    }

    #[inline]
    pub fn get(&self, side: voxel::Side, pos: IVec3) -> Option<T> {
        if let Some(side_vec) = &self.0[side as usize] {
            let index = Self::to_index(side, pos);
            Some(side_vec[index])
        } else {
            None
        }
    }

    #[inline]
    fn to_index(side: voxel::Side, pos: IVec3) -> usize {
        use voxel::Side;

        let check = match &side {
            Side::Right => pos.x == 0,
            Side::Left => pos.x == X_END,
            Side::Up => pos.y == 0,
            Side::Down => pos.y == Y_END,
            Side::Front => pos.z == 0,
            Side::Back => pos.z == Z_END,
        };

        if !check {
            panic!("Invalid {pos}");
        }

        match side {
            Side::Right | Side::Left => (pos.z << Z_SHIFT | pos.y << Y_SHIFT) as usize,
            Side::Up | Side::Down => (pos.x << Z_SHIFT | pos.z << Y_SHIFT) as usize,
            Side::Front | Side::Back => (pos.x << Z_SHIFT | pos.y << Y_SHIFT) as usize,
        }
    }
}

impl<T: ChunkStorageType> PartialEq for ChunkNeighborhood<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[inline]
pub fn overlap_voxel(pos: IVec3) -> (IVec3, IVec3) {
    let overlapping_voxel = math::euclid_rem(
        pos,
        IVec3::new(X_AXIS_SIZE as i32, Y_AXIS_SIZE as i32, Z_AXIS_SIZE as i32),
    );

    let overlapping_dir = (
        if pos.x < 0 {
            -1
        } else if pos.x >= X_AXIS_SIZE as i32 {
            1
        } else {
            0
        },
        if pos.y < 0 {
            -1
        } else if pos.y >= Y_AXIS_SIZE as i32 {
            1
        } else {
            0
        },
        if pos.z < 0 {
            -1
        } else if pos.z >= Z_AXIS_SIZE as i32 {
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
    use bevy_math::IVec3;
    use rand::{random, Rng};

    use super::*;

    #[test]
    fn from_index() {
        assert_eq!(IVec3::new(0, 0, 0), super::from_index(0));
        assert_eq!(IVec3::new(0, 1, 0), super::from_index(1));
        assert_eq!(IVec3::new(0, 2, 0), super::from_index(2));

        assert_eq!(
            IVec3::new(0, 0, 1),
            super::from_index(super::Y_AXIS_SIZE),
            "X >> Z >> Y, so one Z unit should be a full Y axis"
        );
        assert_eq!(
            IVec3::new(0, 1, 1),
            super::from_index(super::Y_AXIS_SIZE + 1)
        );
        assert_eq!(
            IVec3::new(0, 2, 1),
            super::from_index(super::Y_AXIS_SIZE + 2)
        );

        assert_eq!(
            IVec3::new(1, 0, 0),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE)
        );
        assert_eq!(
            IVec3::new(1, 1, 0),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + 1)
        );
        assert_eq!(
            IVec3::new(1, 2, 0),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + 2)
        );

        assert_eq!(
            IVec3::new(1, 0, 1),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + super::Y_AXIS_SIZE)
        );
        assert_eq!(
            IVec3::new(1, 1, 1),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + super::Y_AXIS_SIZE + 1)
        );
        assert_eq!(
            IVec3::new(1, 2, 1),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + super::Y_AXIS_SIZE + 2)
        );
    }

    #[test]
    fn to_index() {
        assert_eq!(super::to_index((0, 0, 0).into()), 0);
        assert_eq!(super::to_index((0, 1, 0).into()), 1);
        assert_eq!(super::to_index((0, 2, 0).into()), 2);

        assert_eq!(super::to_index((0, 0, 1).into()), super::Y_AXIS_SIZE);
        assert_eq!(super::to_index((0, 1, 1).into()), super::Y_AXIS_SIZE + 1);
        assert_eq!(super::to_index((0, 2, 1).into()), super::Y_AXIS_SIZE + 2);

        assert_eq!(
            super::to_index((1, 0, 0).into()),
            super::Y_AXIS_SIZE * super::Z_AXIS_SIZE
        );
        assert_eq!(
            super::to_index((1, 1, 0).into()),
            super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + 1
        );
        assert_eq!(
            super::to_index((1, 2, 0).into()),
            super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + 2
        );

        assert_eq!(
            super::to_index((1, 0, 1).into()),
            super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + super::Y_AXIS_SIZE
        );
        assert_eq!(
            super::to_index((1, 1, 1).into()),
            super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + super::Y_AXIS_SIZE + 1
        );
        assert_eq!(
            super::to_index((1, 2, 1).into()),
            super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + super::Y_AXIS_SIZE + 2
        );
    }

    #[test]
    fn to_index_2d() {
        assert_eq!(super::to_index_2d((0, 0).into()), 0);
        assert_eq!(super::to_index_2d((0, 1).into()), 1);
        assert_eq!(super::to_index_2d((0, 2).into()), 2);

        assert_eq!(super::to_index_2d((1, 0).into()), super::Z_AXIS_SIZE);
        assert_eq!(super::to_index_2d((1, 1).into()), super::Z_AXIS_SIZE + 1);
        assert_eq!(super::to_index_2d((1, 2).into()), super::Z_AXIS_SIZE + 2);
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

            // To world just convert from local chunk coordinates (1, 2, -1) to world coordinates
            // (16, 32, -16) assuming AXIS_SIZE = 16
            assert_eq!(
                base.as_vec3()
                    * Vec3::new(X_AXIS_SIZE as f32, Y_AXIS_SIZE as f32, Z_AXIS_SIZE as f32),
                super::to_world(base)
            );
        }
    }

    #[test]
    fn to_local() {
        use super::*;

        assert_eq!(
            IVec3::new(0, -1, -2),
            super::to_local(Vec3::new(3.0, -0.8, -super::Z_END as f32 - 2.0))
        );
        assert_eq!(
            IVec3::new(0, -1, 0),
            super::to_local(Vec3::new(3.0, -super::Y_END as f32 - 0.8, 0.0))
        );

        const TEST_COUNT: usize = 1000;
        const MAG: f32 = 100.0;

        for _ in 0..TEST_COUNT {
            let base = IVec3::new(
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
                (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
            );

            // This fragment is just used to check if rounding will be correct, since it should not
            // affect the overall chunk local position
            let frag = Vec3::new(
                random::<f32>() * (X_AXIS_SIZE - 1) as f32,
                random::<f32>() * (Y_AXIS_SIZE - 1) as f32,
                random::<f32>() * (Z_AXIS_SIZE - 1) as f32,
            );

            let world = Vec3::new(
                (base.x * X_AXIS_SIZE as i32) as f32 + frag.x,
                (base.y * Y_AXIS_SIZE as i32) as f32 + frag.y,
                (base.z * Z_AXIS_SIZE as i32) as f32 + frag.z,
            );

            // To local convert from world chunk coordinates (15.4, 1.1, -0.5) to local coordinates
            // (1, 0, -1) assuming AXIS_SIZE = 16
            assert_eq!(base, super::to_local(world));
        }
    }

    #[test]
    fn into_iter() {
        let mut first = None;
        let mut last = IVec3::ZERO;

        for pos in super::voxels() {
            assert!(pos.x >= 0 && pos.x < super::X_AXIS_SIZE as i32);
            assert!(pos.y >= 0 && pos.y < super::Y_AXIS_SIZE as i32);
            assert!(pos.z >= 0 && pos.z < super::Z_AXIS_SIZE as i32);

            if first.is_none() {
                first = Some(pos);
            }
            last = pos;
        }

        assert_eq!(first, Some(IVec3::ZERO));
        assert_eq!(
            last,
            (
                X_AXIS_SIZE as i32 - 1,
                Y_AXIS_SIZE as i32 - 1,
                Z_AXIS_SIZE as i32 - 1
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
            ((-1, 0, 0).into(), (super::X_END, 10, 5).into())
        );
        assert_eq!(
            super::overlap_voxel((-1, 10, super::Z_END + 1).into()),
            ((-1, 0, 1).into(), (super::X_END, 10, 0).into())
        );
        assert_eq!(
            super::overlap_voxel((0, 0, 0).into()),
            ((0, 0, 0).into(), (0, 0, 0).into())
        );
        assert_eq!(
            super::overlap_voxel((super::Y_END + 2, 10, 5).into()),
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
                    rnd.gen_range(0..X_AXIS_SIZE) as i32,
                    rnd.gen_range(0..Y_AXIS_SIZE) as i32,
                    rnd.gen_range(0..Z_AXIS_SIZE) as i32,
                )
                    .into();

                match side {
                    Side::Right => pos.x = 0,
                    Side::Left => pos.x = X_END,
                    Side::Up => pos.y = 0,
                    Side::Down => pos.y = Y_END,
                    Side::Front => pos.z = 0,
                    Side::Back => pos.z = Z_END,
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
        assert!(!super::is_at_bounds(local));

        let local = (1, 0, 1).into();
        assert!(super::is_at_bounds(local));

        let local = (1, Y_END, 1).into();
        assert!(super::is_at_bounds(local));

        let local = (0, 0, 0).into();
        assert!(super::is_at_bounds(local));

        let local = (2, 1, 14).into();
        assert!(!super::is_at_bounds(local));
    }

    #[test]
    fn get_boundary_dir() {
        let local = (0, 0, 0).into();
        assert_eq!(super::get_boundary_dir(local), (-1, -1, -1).into());

        let local = (1, 2, 3).into();
        assert_eq!(super::get_boundary_dir(local), (0, 0, 0).into());

        let local = (X_END, 2, 3).into();
        assert_eq!(super::get_boundary_dir(local), (1, 0, 0).into());

        let local = (X_END, Y_END, Z_END).into();
        assert_eq!(super::get_boundary_dir(local), (1, 1, 1).into());
    }

    #[test]
    fn neighboring() {
        let local = (0, 0, 0).into();
        let voxel = (0, 0, 0).into();
        let neighbors = super::neighboring(local, voxel);

        assert_eq!(
            neighbors,
            vec![(-1, 0, 0).into(), (0, -1, 0).into(), (0, 0, -1).into()],
            "Voxel on the edge should return 3 neighbors"
        );
    }

    #[test]
    fn neighboring_empty() {
        let local = (0, 0, 0).into();
        let voxel = (1, 1, 1).into();
        let neighbors = super::neighboring(local, voxel);

        assert!(
            neighbors.is_empty(),
            "Voxel isn't on the edge, so no neighbor should be returned"
        );
    }
}
