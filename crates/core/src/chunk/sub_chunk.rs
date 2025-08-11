#![allow(unused)]
use std::ops::{Deref, DerefMut};

use bevy::{
    math::{IVec3, Vec3},
    platform::collections::HashSet,
    prelude::{Deref, DerefMut},
};
use serde::{Deserialize, Serialize};

use crate::{
    chunk::{ChunkStorageType, sub_chunk},
    voxel::{self, FacesOcclusion, FacesSoftLight, Kind, Light, LightTy, Voxel},
};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SubChunkStorage<T>(Vec<ChunkPack<T>>);

impl<T> SubChunkStorage<T> {
    const SUB_CHUNKS_X: usize = super::X_AXIS_SIZE / pack_consts::X_AXIS_SIZE;
    const SUB_CHUNKS_Y: usize = super::Y_AXIS_SIZE / pack_consts::Y_AXIS_SIZE;
    const SUB_CHUNKS_Z: usize = super::Z_AXIS_SIZE / pack_consts::Z_AXIS_SIZE;

    const SUB_CHUNKS_BUFFER_SIZE: usize =
        Self::SUB_CHUNKS_X * Self::SUB_CHUNKS_Y * Self::SUB_CHUNKS_Z;

    const X_SHIFT: usize = (Self::SUB_CHUNKS_Z.ilog2() + Self::Z_SHIFT as u32) as usize;
    const Z_SHIFT: usize = Self::SUB_CHUNKS_Y.ilog2() as usize;
    const Y_SHIFT: usize = 0;

    const X_MASK: usize = (Self::SUB_CHUNKS_X - 1) << Self::X_SHIFT;
    const Z_MASK: usize = (Self::SUB_CHUNKS_Z - 1) << Self::Z_SHIFT;
    const Y_MASK: usize = Self::SUB_CHUNKS_Y - 1;

    const SUB_CHUNK_DIM: IVec3 = IVec3::new(
        pack_consts::X_AXIS_SIZE as i32,
        pack_consts::Y_AXIS_SIZE as i32,
        pack_consts::Z_AXIS_SIZE as i32,
    );

    #[inline]
    fn to_index(voxel: Voxel) -> usize {
        (voxel.x << Self::X_SHIFT | voxel.z << Self::Z_SHIFT | voxel.y << Self::Y_SHIFT) as usize
    }
}

impl<T> SubChunkStorage<T>
where
    T: Default + Copy,
{
    fn new() -> Self {
        SubChunkStorage(vec![ChunkPack::default(); Self::SUB_CHUNKS_BUFFER_SIZE])
    }
}

impl<T> std::fmt::Debug for SubChunkStorage<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s_cnt = 0;
        let mut p_cnt = 0;
        let mut d_cnt = 0;
        for pack in &self.0 {
            match pack {
                ChunkPack::Single(_) => s_cnt += 1,
                ChunkPack::Pallet { .. } => p_cnt += 1,
                ChunkPack::Dense(_) => d_cnt += 1,
            }
        }
        f.write_fmt(format_args!("S: {s_cnt}, P: {p_cnt}, D: {d_cnt}"))
    }
}

impl<T> Default for SubChunkStorage<T>
where
    T: Default + Copy,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> std::ops::Index<Voxel> for SubChunkStorage<T> {
    type Output = ChunkPack<T>;

    fn index(&self, voxel: Voxel) -> &Self::Output {
        &self.0[Self::to_index(voxel)]
    }
}

impl<T> std::ops::IndexMut<Voxel> for SubChunkStorage<T> {
    fn index_mut(&mut self, voxel: Voxel) -> &mut Self::Output {
        &mut self.0[Self::to_index(voxel)]
    }
}

impl<T> SubChunkStorage<T>
where
    T: ChunkStorageType,
{
    pub fn get(&self, voxel: Voxel) -> T {
        let sub_chunk = voxel / Self::SUB_CHUNK_DIM;
        let sub_voxel = voxel % Self::SUB_CHUNK_DIM;
        self[sub_chunk].get(sub_voxel)
    }

    pub fn set(&mut self, voxel: Voxel, value: T) {
        let sub_chunk = voxel / Self::SUB_CHUNK_DIM;
        let sub_voxel = voxel % Self::SUB_CHUNK_DIM;
        self[sub_chunk].set(sub_voxel, value);
    }

    pub fn is_default(&self) -> bool {
        self.0
            .iter()
            .all(|pack| matches!(pack, ChunkPack::Single(_)))
    }

    pub fn pack(&mut self) {
        self.0.iter_mut().for_each(|p| p.pack());
    }

    pub fn all<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> bool + Copy,
    {
        self.0.iter().all(|pack| pack.all(f))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct VoxelStatePallet<T> {
    pallet: Vec<T>,
    dirty: bool,
}

impl<T> VoxelStatePallet<T>
where
    T: ChunkStorageType,
{
    fn new(pallet: Vec<T>) -> Self {
        Self {
            pallet,
            dirty: false,
        }
    }

    fn empty() -> Self {
        Self {
            pallet: vec![],
            dirty: false,
        }
    }

    fn find_or_add(&mut self, value: T) -> u8 {
        if let Some(index) = self.pallet.iter().position(|s| *s == value) {
            index as u8
        } else {
            self.pallet.push(value);
            self.dirty = true;
            (self.pallet.len() - 1) as u8
        }
    }
}

impl<T> Default for VoxelStatePallet<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            pallet: vec![Default::default()],
            dirty: Default::default(),
        }
    }
}

impl<T> Deref for VoxelStatePallet<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.pallet
    }
}

#[derive(Clone, Debug, PartialEq, Deref, DerefMut, Serialize, Deserialize)]
#[repr(transparent)]
pub(crate) struct PackIndices(pub(crate) Vec<u8>);

#[derive(Clone, Debug, PartialEq, Deref, DerefMut, Serialize, Deserialize)]
#[repr(transparent)]
pub(crate) struct DenseBuffer<T>(pub(crate) Vec<T>);

#[allow(private_interfaces)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum ChunkPack<T> {
    Single(T),
    Pallet {
        pallet: VoxelStatePallet<T>,
        indices: PackIndices,
    },
    Dense(DenseBuffer<T>),
}

mod pack_consts {
    pub(super) const X_AXIS_SIZE: usize = 8;
    pub(super) const Y_AXIS_SIZE: usize = 8;
    pub(super) const Z_AXIS_SIZE: usize = 8;

    pub(super) const X_END: i32 = (X_AXIS_SIZE - 1) as i32;
    pub(super) const Y_END: i32 = (Y_AXIS_SIZE - 1) as i32;
    pub(super) const Z_END: i32 = (Z_AXIS_SIZE - 1) as i32;

    pub(super) const BUFFER_SIZE: usize = X_AXIS_SIZE * Z_AXIS_SIZE * Y_AXIS_SIZE;

    pub(super) const X_SHIFT: usize = (Z_AXIS_SIZE.ilog2() + Z_SHIFT as u32) as usize;
    pub(super) const Z_SHIFT: usize = Y_AXIS_SIZE.ilog2() as usize;
    pub(super) const Y_SHIFT: usize = 0;

    pub(super) const X_MASK: usize = (X_AXIS_SIZE - 1) << X_SHIFT;
    pub(super) const Z_MASK: usize = (Z_AXIS_SIZE - 1) << Z_SHIFT;
    pub(super) const Y_MASK: usize = Y_AXIS_SIZE - 1;
}

impl<T> ChunkPack<T> {
    #[inline]
    fn to_index(voxel: Voxel) -> usize {
        (voxel.x << pack_consts::X_SHIFT
            | voxel.y << pack_consts::Y_SHIFT
            | voxel.z << pack_consts::Z_SHIFT) as usize
    }

    #[inline]
    fn from_index(index: usize) -> Voxel {
        Voxel::new(
            ((index & pack_consts::X_MASK) >> pack_consts::X_SHIFT) as i32,
            ((index & pack_consts::Y_MASK) >> pack_consts::Y_SHIFT) as i32,
            ((index & pack_consts::Z_MASK) >> pack_consts::Z_SHIFT) as i32,
        )
    }
}

impl<T> Default for ChunkPack<T>
where
    T: Default,
{
    fn default() -> Self {
        ChunkPack::Single(Default::default())
    }
}

impl<T> ChunkPack<T>
where
    T: ChunkStorageType,
{
    fn new_pallet() -> Self {
        Self::Pallet {
            pallet: VoxelStatePallet::default(),
            indices: PackIndices(vec![0; pack_consts::BUFFER_SIZE]),
        }
    }

    #[inline]
    fn take(&mut self) -> ChunkPack<T> {
        std::mem::take(self)
    }

    #[inline]
    fn replace(&mut self, mut new: ChunkPack<T>) -> ChunkPack<T> {
        std::mem::replace(self, new)
    }

    pub(crate) fn get(&self, voxel: Voxel) -> T {
        match &self {
            ChunkPack::Single(value) => *value,
            ChunkPack::Pallet { pallet, indices } => {
                pallet[indices[Self::to_index(voxel)] as usize]
            }
            ChunkPack::Dense(voxels) => voxels[Self::to_index(voxel)],
        }
    }

    pub(crate) fn set(&mut self, voxel: Voxel, value: T) {
        if let ChunkPack::Single(current) = self
            && *current == value
        {
            // nothing to do here
            return;
        }

        let new_value = match self.take() {
            ChunkPack::Single(voxel_value) => single_to_pallet(voxel_value, voxel, value),
            ChunkPack::Pallet {
                mut pallet,
                mut indices,
            } => {
                if pallet.len() < u8::MAX as usize || pallet_clean_up(&mut pallet, &mut indices) {
                    let pallet_index = pallet.find_or_add(value);
                    indices[Self::to_index(voxel)] = pallet_index;
                    ChunkPack::Pallet { pallet, indices }
                } else {
                    let mut dense = pallet_to_dense(pallet, indices);
                    dense.set(voxel, value);
                    dense
                }
            }
            ChunkPack::Dense(mut voxels) => {
                voxels[Self::to_index(voxel)] = value;
                ChunkPack::Dense(voxels)
            }
        };
        self.replace(new_value);
    }

    pub(crate) fn pack(&mut self) {
        if matches!(self, ChunkPack::Single(_)) {
            // nothing to do there
            return;
        }

        let new_value = match self.take() {
            ChunkPack::Pallet {
                mut pallet,
                mut indices,
            } => {
                pallet_clean_up(&mut pallet, &mut indices);

                if pallet.len() == 1 {
                    ChunkPack::Single(pallet[0])
                } else {
                    ChunkPack::Pallet { pallet, indices }
                }
            }
            ChunkPack::Dense(voxels) => {
                let uniques = voxels.iter().collect::<HashSet<_>>().len();

                assert!(uniques > 0, "There always be at least 1 unique value");

                if uniques == 1 {
                    ChunkPack::Single(voxels[0])
                } else if uniques <= u8::MAX as usize {
                    dense_to_pallet(voxels)
                } else {
                    ChunkPack::Dense(voxels)
                }
            }
            _ => unreachable!(),
        };

        self.replace(new_value);
    }

    pub(crate) fn all<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> bool,
    {
        match self {
            ChunkPack::Single(v) => f(v),
            ChunkPack::Pallet { pallet, indices } => {
                // If pallet isn't dirty, we can trust the pallet items is all used values.
                if !pallet.dirty {
                    pallet.iter().all(f)
                } else {
                    indices.0.iter().all(|idx| f(&pallet[*idx as usize]))
                }
            }
            ChunkPack::Dense(dense_buffer) => dense_buffer.iter().all(f),
        }
    }
}

fn single_to_pallet<T>(single: T, new_voxel: Voxel, new_value: T) -> ChunkPack<T>
where
    T: ChunkStorageType,
{
    let mut pallet = VoxelStatePallet::new(vec![single, new_value]);
    pallet.dirty = true;

    // init indices point to existing voxel state on pallet
    let mut indices = PackIndices(vec![0; pack_consts::BUFFER_SIZE]);

    // the new voxel state voxel and the second on the pallet
    indices[ChunkPack::<T>::to_index(new_voxel)] = 1;

    ChunkPack::Pallet { pallet, indices }
}

fn pallet_clean_up<T>(pallet: &mut VoxelStatePallet<T>, indices: &mut [u8]) -> bool
where
    T: ChunkStorageType,
{
    if !pallet.dirty {
        return false;
    }
    pallet.dirty = false;

    let mut new_indices = [0u8; pack_consts::BUFFER_SIZE];
    let mut new_pallet = VoxelStatePallet::empty();

    for i in 0..pack_consts::BUFFER_SIZE {
        let new_idx = new_pallet.find_or_add(pallet[indices[i] as usize]);
        new_indices[i] = new_idx;
    }

    if new_pallet.len() < pallet.len() {
        std::mem::swap(pallet, &mut new_pallet);
        indices.copy_from_slice(&new_indices);
        true
    } else {
        false
    }
}

fn pallet_to_dense<T>(pallet: VoxelStatePallet<T>, indices: PackIndices) -> ChunkPack<T>
where
    T: ChunkStorageType,
{
    let mut voxels = DenseBuffer(vec![Default::default(); pack_consts::BUFFER_SIZE]);

    for i in 0..pack_consts::BUFFER_SIZE {
        voxels[i] = pallet[indices[i] as usize];
    }

    ChunkPack::Dense(voxels)
}

fn dense_to_pallet<T>(voxels: DenseBuffer<T>) -> ChunkPack<T>
where
    T: ChunkStorageType,
{
    let mut pallet = VoxelStatePallet::empty();

    let mut indices = PackIndices(vec![0; pack_consts::BUFFER_SIZE]);

    voxels.iter().enumerate().for_each(|(i, value)| {
        let pallet_index = pallet.find_or_add(*value);
        indices[i] = pallet_index;
    });

    ChunkPack::Pallet { pallet, indices }
}

#[cfg(test)]
mod tests {
    use super::{pack_consts::*, *};

    #[test]
    fn all() {
        // Arrange
        let mut single = ChunkPack::Single(u16::MAX);
        let mut pallet = ChunkPack::Single(u16::MAX);
        let mut dense = ChunkPack::Single(u16::MAX);

        for i in 0..BUFFER_SIZE {
            let voxel = ChunkPack::<u8>::from_index(i);

            single.set(voxel, 123u16);
            pallet.set(voxel, i as u16 % 100u16);
            dense.set(voxel, i as u16);
        }

        // Act

        // Assert
        assert!(single.all(|v| *v == 123u16));
        assert!(pallet.all(|v| *v < u8::MAX as u16));
        assert!(dense.all(|v| *v < BUFFER_SIZE as u16));
    }

    #[test]
    fn single_get() {
        // Arrange
        let state = 1u8;
        let single = ChunkPack::Single(state);

        // Act
        let value = single.get(Voxel::new(0, 1, 2));

        // Assert
        assert_eq!(value, state);
    }

    #[test]
    fn single_set_same() {
        // Arrange
        let state = 1u8;
        let mut single = ChunkPack::Single(state);

        // Act
        single.set(Voxel::new(1, 2, 3), state);

        // Assert
        assert!(matches!(single, ChunkPack::Single(_)));
        assert_eq!(single.get(Voxel::new(0, 1, 2)), state);
    }

    #[test]
    fn single_set_diff() {
        // Arrange
        let state = 10u8;
        let diff_state = 2u8;
        let mut chunk = ChunkPack::Single(state);

        // Act
        chunk.set(Voxel::new(1, 2, 3), diff_state);

        // Assert
        assert!(matches!(chunk, ChunkPack::Pallet { .. }));

        let new_state = chunk.get(Voxel::new(1, 2, 3));
        assert_eq!(new_state, diff_state);

        let existing_state = chunk.get(Voxel::new(5, 1, 5));
        assert_eq!(existing_state, state);
    }

    #[test]
    fn pallet_get_set_unique() {
        // Arrange
        let mut pallet = ChunkPack::new_pallet();

        let voxels = [
            Voxel::new(1, 2, 3),
            Voxel::new(4, 5, 6),
            Voxel::new(7, 0, 1),
        ];

        let states = [1u8, 3u8, 4u8];

        // Act
        pallet.set(voxels[0], states[0]);
        pallet.set(voxels[1], states[1]);
        pallet.set(voxels[2], states[2]);

        // Assert
        assert_eq!(pallet.get(voxels[0]), states[0]);
        assert_eq!(pallet.get(voxels[1]), states[1]);
        assert_eq!(pallet.get(voxels[2]), states[2]);
    }

    #[test]
    fn pallet_get_set_non_unique() {
        // Arrange
        let mut pallet = ChunkPack::new_pallet();

        let voxels = [
            Voxel::new(1, 2, 3),
            Voxel::new(3, 5, 6),
            Voxel::new(2, 5, 0),
            Voxel::new(4, 6, 6),
            Voxel::new(7, 0, 1),
        ];

        let states = [1u8, 2u8, 2u8, 4u8, 4u8];

        // Act
        for (idx, voxel) in voxels.into_iter().enumerate() {
            pallet.set(voxel, states[idx]);
        }

        // Assert
        for (idx, voxel) in voxels.into_iter().enumerate() {
            assert_eq!(pallet.get(voxel), states[idx]);
        }

        match pallet {
            // 3 new unique states + default state (air)
            ChunkPack::Pallet { pallet, .. } => assert_eq!(pallet.len(), 4),
            _ => unreachable!("Pallet was changed somewhere"),
        }
    }

    #[test]
    fn pallet_get_set_no_overflow() {
        // Arrange
        let mut pallet = ChunkPack::new_pallet();

        // Act
        for i in 0..u8::MAX as usize {
            pallet.set(ChunkPack::<u8>::from_index(i), i as u8);
        }

        // Assert
        for i in 0..u8::MAX as usize {
            assert_eq!(pallet.get(ChunkPack::<u8>::from_index(i)), i as u8);
        }

        match pallet {
            ChunkPack::Pallet { pallet, .. } => assert_eq!(pallet.len(), u8::MAX as usize),
            _ => unreachable!("Pallet was changed somewhere"),
        }
    }

    #[test]
    fn pallet_get_set_no_overflow_with_dead_state() {
        // Arrange
        let mut pallet = ChunkPack::new_pallet();

        // insert some dead states
        match &mut pallet {
            ChunkPack::Pallet { pallet, indices } => {
                for i in 0u8..20 {
                    pallet.pallet.push(1000 + i as u16);
                }
            }
            _ => unreachable!(),
        }

        // Act
        for i in 0..u8::MAX as usize {
            pallet.set(ChunkPack::<u8>::from_index(i), i as u16);
        }

        // Assert
        for i in 0..u8::MAX as usize {
            assert_eq!(pallet.get(ChunkPack::<u8>::from_index(i)), i as u16);
        }

        match pallet {
            ChunkPack::Pallet { pallet, .. } => assert_eq!(pallet.len(), u8::MAX as usize),
            _ => unreachable!("Pallet was changed somewhere"),
        }
    }

    #[test]
    fn pallet_get_set_overflow() {
        // Arrange
        let mut pallet = ChunkPack::new_pallet();

        // Act
        for i in 0..BUFFER_SIZE {
            pallet.set(ChunkPack::<u8>::from_index(i), i as u16);
        }

        // Assert
        for i in 0..BUFFER_SIZE {
            assert_eq!(pallet.get(ChunkPack::<u8>::from_index(i)), i as u16);
        }

        assert!(matches!(pallet, ChunkPack::Dense(_)));
    }

    #[test]
    fn pallet_get_set_overflow_with_dead_states() {
        // Arrange
        let mut pallet = ChunkPack::new_pallet();

        // insert some dead states
        match &mut pallet {
            ChunkPack::Pallet { pallet, indices } => {
                for i in 0u8..20 {
                    pallet.pallet.push(1000 + i as u16);
                }
            }
            _ => unreachable!(),
        }

        // Act
        for i in 0..BUFFER_SIZE {
            pallet.set(ChunkPack::<u8>::from_index(i), i as u16);
        }

        // Assert
        for i in 0..BUFFER_SIZE {
            assert_eq!(pallet.get(ChunkPack::<u8>::from_index(i)), i as u16);
        }

        assert!(matches!(pallet, ChunkPack::Dense(_)));
    }

    #[test]
    fn dense_get_set() {
        // Arrange
        let mut pack = ChunkPack::Dense(DenseBuffer(vec![Default::default(); BUFFER_SIZE]));
        let state = 1u8;
        let voxel = Voxel::new(1, 2, 3);

        // Act
        pack.set(voxel, state);

        // Assert
        assert_eq!(state, pack.get(voxel));
    }

    #[test]
    fn pack_single() {
        // Arrange
        let state = 1u8;
        let mut single = ChunkPack::Single(state);

        // Act
        single.pack();

        // Assert
        match single {
            ChunkPack::Single(voxel_state) => assert_eq!(voxel_state, state),
            _ => panic!("Calling pack on single value should never change it"),
        }
    }

    #[test]
    fn pack_pallet_with_one_unique() {
        // Arrange
        let state = 1u8;

        let mut pallet = ChunkPack::Pallet {
            pallet: VoxelStatePallet::new(vec![state]),
            indices: PackIndices(vec![0; BUFFER_SIZE]),
        };

        // Act
        pallet.pack();

        // Assert
        match pallet {
            ChunkPack::Single(voxel_state) => assert_eq!(voxel_state, state),
            _ => panic!("Calling pack on pallet with 1 unique value should return single"),
        }
    }

    #[test]
    fn pack_pallet_with_one_unique_with_dead_state() {
        // Arrange
        let state = 1u8;
        let dead = 2u8;

        let mut pallet = VoxelStatePallet::new(vec![state, dead]);
        pallet.dirty = true;

        let mut pallet = ChunkPack::Pallet {
            pallet,
            indices: PackIndices(vec![0; BUFFER_SIZE]),
        };

        // Act
        pallet.pack();

        // Assert
        match pallet {
            ChunkPack::Single(voxel_state) => assert_eq!(voxel_state, state),
            _ => panic!("Calling pack on pallet with 1 unique value should return single"),
        }
    }

    #[test]
    fn pack_dense_unique() {
        // Arrange
        let mut voxels = DenseBuffer(vec![Default::default(); BUFFER_SIZE]);
        for i in 0..BUFFER_SIZE {
            voxels[i] = i as u16;
        }

        let mut dense = ChunkPack::Dense(voxels);
        // Act
        dense.pack();

        // Assert
        assert!(
            matches!(dense, ChunkPack::Dense(_)),
            "The pack is full of unique states, so there should be no change"
        );
        for i in 0..BUFFER_SIZE {
            assert_eq!(dense.get(ChunkPack::<u8>::from_index(i)), i as u16);
        }
    }

    #[test]
    fn pack_dense_non_unique() {
        // Arrange
        let mut voxels = DenseBuffer(vec![Default::default(); BUFFER_SIZE]);
        for i in 0..BUFFER_SIZE {
            let v = (i % 255) as u8;
            voxels[i] = v;
        }

        let mut dense = ChunkPack::Dense(voxels);

        // Act
        dense.pack();

        // Assert
        match &dense {
            ChunkPack::Pallet { pallet, .. } => {
                assert_eq!(pallet.len(), 255);
            }
            _ => unreachable!("This should be packed into a pallet"),
        }

        for i in 0..BUFFER_SIZE {
            let v = (i % 255) as u8;
            assert_eq!(dense.get(ChunkPack::<u8>::from_index(i)), v);
        }
    }
}
