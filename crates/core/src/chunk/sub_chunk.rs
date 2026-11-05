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
    coords::{Chunk, ChunkVoxel, Voxel},
    voxel::{self, FacesOcclusion, FacesSoftLight, Kind, Light, LightTy},
};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub(crate) struct PackVoxel {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl PackVoxel {
    pub fn new(x: u8, y: u8, z: u8) -> Self {
        Self {
            x: x & (pack_consts::X_AXIS_SIZE - 1) as u8,
            y: y & (pack_consts::Y_AXIS_SIZE - 1) as u8,
            z: z & (pack_consts::Z_AXIS_SIZE - 1) as u8,
        }
    }

    fn from_index(index: usize) -> Self {
        Self::new(
            ((index & pack_consts::X_MASK) >> pack_consts::X_SHIFT) as u8,
            ((index & pack_consts::Y_MASK) >> pack_consts::Y_SHIFT) as u8,
            ((index & pack_consts::Z_MASK) >> pack_consts::Z_SHIFT) as u8,
        )
    }

    pub fn to_index(self) -> usize {
        (self.x as usize) << pack_consts::X_SHIFT
            | (self.y as usize) << pack_consts::Y_SHIFT
            | (self.z as usize) << pack_consts::Z_SHIFT
    }
}

impl From<ChunkVoxel> for PackVoxel {
    fn from(value: ChunkVoxel) -> Self {
        let x = value.x % pack_consts::X_AXIS_SIZE as u8;
        let y = value.y % pack_consts::Y_AXIS_SIZE as u8;
        let z = value.z % pack_consts::Z_AXIS_SIZE as u8;

        Self::new(x, y, z)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawSubChunkStorage<T>")]
pub(crate) struct SubChunkStorage<T>(Vec<ChunkPack<T>>);

#[derive(Deserialize)]
struct RawSubChunkStorage<T>(Vec<ChunkPack<T>>);

impl<T> TryFrom<RawSubChunkStorage<T>> for SubChunkStorage<T> {
    type Error = String;

    fn try_from(raw: RawSubChunkStorage<T>) -> Result<Self, Self::Error> {
        let expected = (Chunk::X_AXIS_SIZE / pack_consts::X_AXIS_SIZE)
            * (Chunk::Y_AXIS_SIZE / pack_consts::Y_AXIS_SIZE)
            * (Chunk::Z_AXIS_SIZE / pack_consts::Z_AXIS_SIZE);

        if raw.0.len() != expected {
            return Err(format!(
                "section storage must have {} sections, got {}",
                expected,
                raw.0.len()
            ));
        }

        Ok(Self(raw.0))
    }
}

impl<T> SubChunkStorage<T> {
    const SUB_CHUNKS_X: usize = Chunk::X_AXIS_SIZE / pack_consts::X_AXIS_SIZE;
    const SUB_CHUNKS_Y: usize = Chunk::Y_AXIS_SIZE / pack_consts::Y_AXIS_SIZE;
    const SUB_CHUNKS_Z: usize = Chunk::Z_AXIS_SIZE / pack_consts::Z_AXIS_SIZE;

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
    fn get_pack_index(voxel: ChunkVoxel) -> usize {
        let x = voxel.x / pack_consts::X_AXIS_SIZE as u8;
        let y = voxel.y / pack_consts::Y_AXIS_SIZE as u8;
        let z = voxel.z / pack_consts::Z_AXIS_SIZE as u8;

        (x << Self::X_SHIFT | z << Self::Z_SHIFT | y << Self::Y_SHIFT) as usize
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

impl<T> SubChunkStorage<T>
where
    T: ChunkStorageType,
{
    pub fn get(&self, voxel: ChunkVoxel) -> T {
        let pack_index = Self::get_pack_index(voxel);
        self.0[pack_index].get(voxel.into())
    }

    pub fn set(&mut self, voxel: ChunkVoxel, value: T) {
        let pack_index = Self::get_pack_index(voxel);
        self.0[pack_index].set(voxel.into(), value);
    }

    pub fn is_default(&self) -> bool {
        self.0
            .iter()
            .all(|pack| matches!(pack, ChunkPack::Single(_)))
    }

    pub fn fill(&mut self, value: T) {
        self.0.fill(ChunkPack::Single(value));
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

    /// Returns the value of a uniform horizontal layer, or `None` when it is mixed.
    pub fn layer_uniform(&self, y: usize) -> Option<T> {
        if y >= Chunk::Y_AXIS_SIZE {
            return None;
        }

        let y_u8 = y as u8;
        let voxel = ChunkVoxel::new(0, y_u8, 0);

        if let ChunkPack::Single(value) = &self.0[Self::get_pack_index(voxel)] {
            return Some(*value);
        }

        let first = self.get(voxel);

        (0..Chunk::X_AXIS_SIZE)
            .all(|x| {
                (0..Chunk::Z_AXIS_SIZE)
                    .all(|z| self.get(ChunkVoxel::new(x as u8, y_u8, z as u8)) == first)
            })
            .then_some(first)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawVoxelStatePallet<T>")]
struct VoxelStatePallet<T> {
    pallet: Vec<T>,
    dirty: bool,
}

#[derive(Deserialize)]
struct RawVoxelStatePallet<T> {
    pallet: Vec<T>,
    dirty: bool,
}

impl<T> TryFrom<RawVoxelStatePallet<T>> for VoxelStatePallet<T> {
    type Error = String;

    fn try_from(raw: RawVoxelStatePallet<T>) -> Result<Self, Self::Error> {
        if raw.pallet.len() > u8::MAX as usize {
            return Err(format!(
                "a section pallet cannot have more than {} entries, got {}",
                u8::MAX,
                raw.pallet.len()
            ));
        }

        Ok(Self {
            pallet: raw.pallet,
            dirty: raw.dirty,
        })
    }
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPackIndices")]
struct PackIndices {
    bits: u8,
    buffer: Vec<u8>,
}

#[derive(Deserialize)]
struct RawPackIndices {
    bits: u8,
    buffer: Vec<u8>,
}

impl TryFrom<RawPackIndices> for PackIndices {
    type Error = String;

    fn try_from(raw: RawPackIndices) -> Result<Self, Self::Error> {
        if !(1..=8).contains(&raw.bits) {
            return Err(format!(
                "section indices bits must be between 1 and 8, got {}",
                raw.bits
            ));
        }

        let expected = (pack_consts::BUFFER_SIZE * raw.bits as usize).div_ceil(8);
        if raw.buffer.len() != expected {
            return Err(format!(
                "section indices for {} bits must have {} bytes, got {}",
                raw.bits,
                expected,
                raw.buffer.len()
            ));
        }

        Ok(Self {
            bits: raw.bits,
            buffer: raw.buffer,
        })
    }
}

impl PackIndices {
    fn new(pallet_len: usize) -> Self {
        let bits = (pallet_len as f32).log2().ceil().max(1.0) as u8;
        let buffer_size = (pack_consts::BUFFER_SIZE * bits as usize).div_ceil(8);
        Self {
            bits,
            buffer: vec![0; buffer_size],
        }
    }

    fn from_values(values: &[u8], pallet_len: usize) -> Self {
        let mut indices = Self::new(pallet_len);
        for (index, value) in values.iter().enumerate() {
            indices.set(index, *value);
        }
        indices
    }

    fn get(&self, index: usize) -> u8 {
        let bit = index * self.bits as usize;
        let byte = bit / 8;
        let offset = bit % 8;

        let lo = self.buffer[byte] as u16;
        let hi = self.buffer.get(byte + 1).copied().unwrap_or(0) as u16;
        let word = lo | (hi << 8);

        let mask = (1u16 << self.bits as u16) - 1;
        ((word >> offset) & mask) as u8
    }

    fn set(&mut self, index: usize, value: u8) {
        let bit = index * self.bits as usize;
        let byte = bit / 8;
        let offset = bit % 8;

        let lo = self.buffer[byte] as u16;
        let hi = self.buffer.get(byte + 1).copied().unwrap_or(0) as u16;
        let word = lo | (hi << 8);

        let mask = (1u16 << self.bits as u16) - 1;
        let cleared = word & !(mask << offset);
        let new_word = cleared | ((value as u16 & mask) << offset);

        self.buffer[byte] = new_word as u8;
        if byte + 1 < self.buffer.len() {
            self.buffer[byte + 1] = (new_word >> 8) as u8;
        }
    }

    fn iter(&self) -> impl Iterator<Item = u8> + '_ {
        (0..pack_consts::BUFFER_SIZE).map(|index| self.get(index))
    }

    fn needs_to_grow(&self, new_size: u8) -> bool {
        new_size as u16 > 1 << self.bits as u16
    }

    fn grow(&mut self, new_size: u8) {
        let mut new_indices = Self::new(new_size as usize);
        for (index, value) in self.iter().enumerate() {
            new_indices.set(index, value);
        }
        *self = new_indices;
    }
}

#[derive(Clone, Debug, PartialEq, Deref, DerefMut, Serialize, Deserialize)]
#[serde(try_from = "RawDenseBuffer<T>")]
#[repr(transparent)]
pub(crate) struct DenseBuffer<T>(pub(crate) Vec<T>);

#[derive(Deserialize)]
struct RawDenseBuffer<T>(Vec<T>);

impl<T> TryFrom<RawDenseBuffer<T>> for DenseBuffer<T> {
    type Error = String;

    fn try_from(raw: RawDenseBuffer<T>) -> Result<Self, Self::Error> {
        if raw.0.len() != pack_consts::BUFFER_SIZE {
            return Err(format!(
                "section dense buffer must have {} entries, got {}",
                pack_consts::BUFFER_SIZE,
                raw.0.len()
            ));
        }

        Ok(Self(raw.0))
    }
}

#[allow(private_interfaces)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawChunkPack<T>")]
pub(crate) enum ChunkPack<T> {
    Single(T),
    Pallet {
        pallet: VoxelStatePallet<T>,
        indices: PackIndices,
    },
    Dense(DenseBuffer<T>),
}

#[derive(Deserialize)]
enum RawChunkPack<T> {
    Single(T),
    Pallet {
        pallet: VoxelStatePallet<T>,
        indices: PackIndices,
    },
    Dense(DenseBuffer<T>),
}

impl<T> TryFrom<RawChunkPack<T>> for ChunkPack<T> {
    type Error = String;

    fn try_from(raw: RawChunkPack<T>) -> Result<Self, Self::Error> {
        match raw {
            RawChunkPack::Single(value) => Ok(ChunkPack::Single(value)),
            RawChunkPack::Pallet { pallet, indices } => {
                for index in indices.iter() {
                    if index as usize >= pallet.len() {
                        return Err(format!(
                            "section pallet index {} is out of bounds for {} entries",
                            index,
                            pallet.len()
                        ));
                    }
                }

                Ok(ChunkPack::Pallet { pallet, indices })
            }
            RawChunkPack::Dense(voxels) => Ok(ChunkPack::Dense(voxels)),
        }
    }
}

pub(crate) mod pack_consts {
    pub(super) const X_AXIS_SIZE: usize = 16;
    pub(super) const Y_AXIS_SIZE: usize = 16;
    pub(super) const Z_AXIS_SIZE: usize = 16;

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
            indices: PackIndices::new(0),
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

    pub(crate) fn get(&self, voxel: PackVoxel) -> T {
        match &self {
            ChunkPack::Single(value) => *value,
            ChunkPack::Pallet { pallet, indices } => pallet[indices.get(voxel.to_index()) as usize],
            ChunkPack::Dense(voxels) => voxels[voxel.to_index()],
        }
    }

    pub(crate) fn set(&mut self, voxel: PackVoxel, value: T) {
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
                    if indices.needs_to_grow(pallet_index + 1) {
                        indices.grow(pallet_index + 1);
                    }
                    indices.set(voxel.to_index(), pallet_index);
                    ChunkPack::Pallet { pallet, indices }
                } else {
                    let mut dense = pallet_to_dense(pallet, indices);
                    dense.set(voxel, value);
                    dense
                }
            }
            ChunkPack::Dense(mut voxels) => {
                voxels[voxel.to_index()] = value;
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
                    indices.iter().all(|idx| f(&pallet[idx as usize]))
                }
            }
            ChunkPack::Dense(dense_buffer) => dense_buffer.iter().all(f),
        }
    }
}

fn single_to_pallet<T>(single: T, new_voxel: PackVoxel, new_value: T) -> ChunkPack<T>
where
    T: ChunkStorageType,
{
    let mut pallet = VoxelStatePallet::new(vec![single, new_value]);
    pallet.dirty = true;

    // init indices point to existing voxel state on pallet
    let mut indices = PackIndices::new(2);

    // the new voxel state voxel and the second on the pallet
    indices.set(new_voxel.to_index(), 1);

    ChunkPack::Pallet { pallet, indices }
}

fn pallet_clean_up<T>(pallet: &mut VoxelStatePallet<T>, indices: &mut PackIndices) -> bool
where
    T: ChunkStorageType,
{
    if !pallet.dirty {
        return false;
    }
    pallet.dirty = false;

    let mut new_values = Vec::with_capacity(pack_consts::BUFFER_SIZE);
    let mut new_pallet = VoxelStatePallet::empty();

    for i in 0..pack_consts::BUFFER_SIZE {
        let new_idx = new_pallet.find_or_add(pallet[indices.get(i) as usize]);
        new_values.push(new_idx);
    }

    if new_pallet.len() < pallet.len() {
        std::mem::swap(pallet, &mut new_pallet);
        pallet.dirty = false;
        *indices = PackIndices::from_values(&new_values, pallet.len());
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
        voxels[i] = pallet[indices.get(i) as usize];
    }

    ChunkPack::Dense(voxels)
}

fn dense_to_pallet<T>(voxels: DenseBuffer<T>) -> ChunkPack<T>
where
    T: ChunkStorageType,
{
    let mut pallet = VoxelStatePallet::empty();

    let mut indices = PackIndices::new(0);

    voxels.iter().enumerate().for_each(|(i, value)| {
        let pallet_index = pallet.find_or_add(*value);
        if indices.needs_to_grow(pallet_index + 1) {
            indices.grow(pallet_index + 1);
        }
        indices.set(i, pallet_index);
    });

    ChunkPack::Pallet { pallet, indices }
}

#[cfg(test)]
mod tests {
    use super::{pack_consts::*, *};

    #[test]
    fn pack_voxel_from_index() {
        // Arrange

        // Act
        let first = PackVoxel::from_index(0);
        let y_end = PackVoxel::from_index(pack_consts::Y_END as usize);
        let last = PackVoxel::from_index(BUFFER_SIZE - 1);

        // Assert
        assert_eq!(first, PackVoxel::new(0, 0, 0));
        assert_eq!(y_end, PackVoxel::new(0, Y_END as u8, 0));
        assert_eq!(last, PackVoxel::new(X_END as u8, Y_END as u8, Z_END as u8));
    }

    #[test]
    fn pack_voxel_to_index() {
        // Arrange

        // Act
        let first = PackVoxel::new(0, 0, 0).to_index();
        let y_end = PackVoxel::new(0, Y_END as u8, 0).to_index();
        let last = PackVoxel::new(X_END as u8, Y_END as u8, Z_END as u8).to_index();

        // Assert
        assert_eq!(first, 0);
        assert_eq!(y_end, pack_consts::Y_AXIS_SIZE - 1);
        assert_eq!(last, pack_consts::BUFFER_SIZE - 1);
    }

    #[test]
    fn all() {
        // Arrange
        let mut single = ChunkPack::Single(u16::MAX);
        let mut pallet = ChunkPack::Single(u16::MAX);
        let mut dense = ChunkPack::Single(u16::MAX);

        for i in 0..BUFFER_SIZE {
            let voxel = PackVoxel::from_index(i);

            single.set(voxel, 123u16);
            pallet.set(voxel, i as u16 % 100u16);
            dense.set(voxel, i as u16);
        }

        // Act

        // Assert
        assert!(single.all(|v| if *v != 123u16 {
            panic!("Failed at {v:?}.{single:?}");
        } else {
            true
        }));
        assert!(pallet.all(|v| *v < u8::MAX as u16));
        assert!(dense.all(|v| *v < BUFFER_SIZE as u16));
    }

    #[test]
    fn single_get() {
        // Arrange
        let state = 1u8;
        let single = ChunkPack::Single(state);

        // Act
        let value = single.get(PackVoxel::new(0, 1, 2));

        // Assert
        assert_eq!(value, state);
    }

    #[test]
    fn single_set_same() {
        // Arrange
        let state = 1u8;
        let mut single = ChunkPack::Single(state);

        // Act
        single.set(PackVoxel::new(1, 2, 3), state);

        // Assert
        assert!(matches!(single, ChunkPack::Single(_)));
        assert_eq!(single.get(PackVoxel::new(0, 1, 2)), state);
    }

    #[test]
    fn single_set_diff() {
        // Arrange
        let state = 10u8;
        let diff_state = 2u8;
        let mut chunk = ChunkPack::Single(state);

        // Act
        chunk.set(PackVoxel::new(1, 2, 3), diff_state);

        // Assert
        assert!(matches!(chunk, ChunkPack::Pallet { .. }));

        let new_state = chunk.get(PackVoxel::new(1, 2, 3));
        assert_eq!(new_state, diff_state);

        let existing_state = chunk.get(PackVoxel::new(5, 1, 5));
        assert_eq!(existing_state, state);
    }

    #[test]
    fn pallet_get_set_unique() {
        // Arrange
        let mut pallet = ChunkPack::new_pallet();

        let voxels = [
            PackVoxel::new(1, 2, 3),
            PackVoxel::new(4, 5, 6),
            PackVoxel::new(7, 0, 1),
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
            PackVoxel::new(1, 2, 3),
            PackVoxel::new(3, 5, 6),
            PackVoxel::new(2, 5, 0),
            PackVoxel::new(4, 6, 6),
            PackVoxel::new(7, 0, 1),
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
            pallet.set(PackVoxel::from_index(i), i as u8);
        }

        // Assert
        for i in 0..u8::MAX as usize {
            assert_eq!(pallet.get(PackVoxel::from_index(i)), i as u8);
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
            pallet.set(PackVoxel::from_index(i), i as u16);
        }

        // Assert
        for i in 0..u8::MAX as usize {
            assert_eq!(pallet.get(PackVoxel::from_index(i)), i as u16);
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
            pallet.set(PackVoxel::from_index(i), i as u16);
        }

        // Assert
        for i in 0..BUFFER_SIZE {
            assert_eq!(pallet.get(PackVoxel::from_index(i)), i as u16);
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
            pallet.set(PackVoxel::from_index(i), i as u16);
        }

        // Assert
        for i in 0..BUFFER_SIZE {
            assert_eq!(pallet.get(PackVoxel::from_index(i)), i as u16);
        }

        assert!(matches!(pallet, ChunkPack::Dense(_)));
    }

    #[test]
    fn dense_get_set() {
        // Arrange
        let mut pack = ChunkPack::Dense(DenseBuffer(vec![Default::default(); BUFFER_SIZE]));
        let state = 1u8;
        let voxel = PackVoxel::new(1, 2, 3);

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
            indices: PackIndices::new(0),
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
            indices: PackIndices::new(0),
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
            assert_eq!(dense.get(PackVoxel::from_index(i)), i as u16);
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
            assert_eq!(dense.get(PackVoxel::from_index(i)), v);
        }
    }

    fn encode<T: serde::Serialize>(value: &T) -> Vec<u8> {
        bincode::serde::encode_to_vec(value, bincode::config::standard()).unwrap()
    }

    fn decode<T: serde::de::DeserializeOwned>(
        bytes: &[u8],
    ) -> Result<T, bincode::error::DecodeError> {
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())
            .map(|(value, _)| value)
    }

    #[test]
    fn pack_indices_serde_rejects_wrong_len() {
        let invalid = PackIndices {
            bits: 1,
            buffer: vec![0; (BUFFER_SIZE / 8) + 1],
        };
        assert!(decode::<PackIndices>(&encode(&invalid)).is_err());

        let invalid = PackIndices {
            bits: 1,
            buffer: Vec::new(),
        };
        assert!(decode::<PackIndices>(&encode(&invalid)).is_err());
    }

    #[test]
    fn dense_buffer_serde_rejects_wrong_len() {
        assert!(
            decode::<DenseBuffer<u8>>(&encode(&DenseBuffer(vec![0; BUFFER_SIZE + 1]))).is_err()
        );
        assert!(decode::<DenseBuffer<u8>>(&encode(&DenseBuffer::<u8>(Vec::new()))).is_err());
    }

    #[test]
    fn voxel_state_pallet_serde_rejects_too_many_entries() {
        let invalid = VoxelStatePallet {
            pallet: vec![0u16; u8::MAX as usize + 1],
            dirty: false,
        };

        assert!(decode::<VoxelStatePallet<u16>>(&encode(&invalid)).is_err());
    }

    #[test]
    fn pack_indices_serde_round_trip_all_bits() {
        for bits in 1..=8u8 {
            let pallet_len = (1usize << (bits - 1)) + 1;
            let max = ((1u16 << bits) - 1) as u8;

            let mut indices = PackIndices::new(pallet_len);
            indices.set(0, max);
            indices.set(7, max);
            indices.set(8, max.saturating_sub(1));
            indices.set(pack_consts::BUFFER_SIZE - 1, max);

            let decoded: PackIndices = decode(&encode(&indices)).unwrap();

            assert_eq!(decoded.get(0), max);
            assert_eq!(decoded.get(7), max);
            assert_eq!(decoded.get(8), max.saturating_sub(1));
            assert_eq!(decoded.get(pack_consts::BUFFER_SIZE - 1), max);
        }
    }

    #[test]
    fn pack_indices_serde_rejects_invalid_bits() {
        let invalid = PackIndices {
            bits: 0,
            buffer: Vec::new(),
        };
        assert!(decode::<PackIndices>(&encode(&invalid)).is_err());

        let invalid = PackIndices {
            bits: 9,
            buffer: Vec::new(),
        };
        assert!(decode::<PackIndices>(&encode(&invalid)).is_err());
    }

    #[test]
    fn chunk_pack_serde_round_trip_max_palette() {
        let pack = ChunkPack::Pallet {
            pallet: VoxelStatePallet::new((0..255).collect::<Vec<u16>>()),
            indices: PackIndices::new(255),
        };

        let decoded: ChunkPack<u16> = decode(&encode(&pack)).unwrap();

        match decoded {
            ChunkPack::Pallet { pallet, indices } => {
                assert_eq!(pallet.len(), 255);
                assert_eq!(indices.get(0), 0);
            }
            _ => panic!("expected pallet"),
        }
    }

    #[test]
    fn chunk_pack_serde_rejects_out_of_bounds_index() {
        let mut indices = PackIndices::new(3);
        for i in 0..BUFFER_SIZE {
            indices.set(i, 2);
        }
        let invalid = ChunkPack::Pallet {
            pallet: VoxelStatePallet::new(vec![1u16, 2]),
            indices,
        };

        assert!(decode::<ChunkPack<u16>>(&encode(&invalid)).is_err());
    }

    #[test]
    fn sub_chunk_storage_serde_rejects_wrong_section_count() {
        let invalid = SubChunkStorage(vec![ChunkPack::Single(0u16)]);

        assert!(decode::<SubChunkStorage<u16>>(&encode(&invalid)).is_err());
    }

    #[test]
    fn sub_chunk_storage_serde_round_trip() {
        let mut storage = SubChunkStorage::<u16>::default();
        storage.set(ChunkVoxel::new(0, 0, 0), 1);
        storage.set(ChunkVoxel::new(15, 255, 15), 2);
        storage.pack();

        let decoded: SubChunkStorage<u16> = decode(&encode(&storage)).unwrap();

        assert_eq!(decoded.get(ChunkVoxel::new(0, 0, 0)), 1);
        assert_eq!(decoded.get(ChunkVoxel::new(15, 255, 15)), 2);
    }
}
