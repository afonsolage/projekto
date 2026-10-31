#![allow(unused)]
use std::ops::{Deref, DerefMut};

use bevy::{
    math::{IVec2, IVec3, Vec3, Vec3Swizzles},
    platform::collections::HashSet,
    prelude::{Deref, DerefMut},
};
use serde::{Deserialize, Serialize};

use crate::{
    chunk::{self, ChunkStorageType},
    coords::{Chunk, ChunkVoxel, Voxel},
    voxel::{self, FacesOcclusion, FacesSoftLight, Kind, Light, LightTy},
};

const COLUMN_SIZE: usize = Chunk::Y_AXIS_SIZE;
const COLUMN_COUNT: usize = Chunk::X_AXIS_SIZE * Chunk::Z_AXIS_SIZE;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ChunkColumnStorage<T>(Vec<ChunkColumn<T>>);

impl<T> ChunkColumnStorage<T>
where
    T: Default + Copy,
{
    fn new() -> Self {
        ChunkColumnStorage(vec![ChunkColumn::default(); COLUMN_COUNT])
    }
}

impl<T> std::fmt::Debug for ChunkColumnStorage<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s_cnt = 0;
        let mut p_cnt = 0;
        for pack in &self.0 {
            match pack {
                ChunkColumn::Single(_) => s_cnt += 1,
                ChunkColumn::Pallet { .. } => p_cnt += 1,
            }
        }
        f.write_fmt(format_args!("S: {s_cnt}, P: {p_cnt}"))
    }
}

impl<T> Default for ChunkColumnStorage<T>
where
    T: Default + Copy,
{
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkVoxel {
    #[inline(always)]
    fn column_index(&self) -> usize {
        (self.x << 4 | self.z) as usize
    }

    fn from_column_index(index: u8) -> Self {
        Self::new(index >> 4, 0, index & 0x0F)
    }
}

#[inline(always)]
fn column_index(v: IVec3) -> usize {
    (v.x << 4 | v.z) as usize
}

impl<T> ChunkColumnStorage<T>
where
    T: ChunkStorageType,
{
    pub fn get(&self, voxel: ChunkVoxel) -> T {
        self.0[voxel.column_index()].get(voxel.y)
    }

    pub fn set(&mut self, voxel: ChunkVoxel, value: T) {
        self.0[voxel.column_index()].set(voxel.y, value);
    }

    pub fn is_default(&self) -> bool {
        self.0
            .iter()
            .all(|pack| matches!(pack, ChunkColumn::Single(_)))
    }

    pub fn pack(&mut self) {
        self.0.iter_mut().for_each(|p| p.pack());
    }

    pub fn filter<F>(&self, mut f: F) -> Vec<ChunkVoxel>
    where
        F: FnMut(&T) -> bool + Copy,
    {
        self.0.iter().enumerate().fold(
            Vec::with_capacity(Chunk::BUFFER_SIZE / 4),
            |mut voxels, (column_idx, column)| {
                debug_assert!(column_idx < COLUMN_SIZE);

                let base_voxel = ChunkVoxel::from_column_index(column_idx as u8);
                match column {
                    ChunkColumn::Single(v) => {
                        if f(v) {
                            voxels.extend(
                                (0..=(COLUMN_COUNT - 1) as u8)
                                    .map(|y| ChunkVoxel::new(base_voxel.x, y, base_voxel.z)),
                            );
                        }
                    }
                    ChunkColumn::Pallet { pallet, indices } => {
                        for (pallet_idx, v) in pallet.pallet.iter().enumerate() {
                            if f(v) {
                                let target_idx = pallet_idx as u8;
                                let filtered_voxels = indices
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, pallet_idx)| *pallet_idx == target_idx)
                                    .map(|(y_idx, _)| {
                                        ChunkVoxel::new(base_voxel.x, y_idx as u8, base_voxel.z)
                                    });

                                voxels.extend(filtered_voxels);
                            }
                        }
                    }
                }

                voxels
            },
        )
    }

    pub fn all<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> bool + Copy,
    {
        self.0.iter().enumerate().all(|(idx, pack)| pack.all(f))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawVoxelPallet<T>")]
struct VoxelPallet<T> {
    pallet: Vec<T>,
    dirty: bool,
}

#[derive(Deserialize)]
struct RawVoxelPallet<T> {
    pallet: Vec<T>,
    dirty: bool,
}

impl<T> TryFrom<RawVoxelPallet<T>> for VoxelPallet<T> {
    type Error = String;

    fn try_from(raw: RawVoxelPallet<T>) -> Result<Self, Self::Error> {
        if raw.pallet.len() > u8::MAX as usize {
            return Err(format!(
                "a voxel pallet cannot have more than {} entries, got {}",
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

impl<T> VoxelPallet<T>
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

    fn len(&self) -> usize {
        self.pallet.len()
    }

    fn find_or_add(&mut self, value: T) -> u8 {
        if let Some(index) = self.pallet.iter().position(|s| *s == value) {
            index as u8
        } else {
            assert!(
                self.pallet.len() < u8::MAX as usize,
                "A chunk column cannot have more than 255 voxel types"
            );

            self.pallet.push(value);
            self.dirty = true;

            let new_index = self.pallet.len() - 1;

            new_index as u8
        }
    }
}

impl<T> Default for VoxelPallet<T>
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

impl<T> Deref for VoxelPallet<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.pallet
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawIndices")]
pub(crate) struct Indices {
    bits: u8,
    buffer: Vec<u8>,
}

#[derive(Deserialize)]
struct RawIndices {
    bits: u8,
    buffer: Vec<u8>,
}

impl TryFrom<RawIndices> for Indices {
    type Error = String;

    fn try_from(raw: RawIndices) -> Result<Self, Self::Error> {
        if !(1..=8).contains(&raw.bits) {
            return Err(format!(
                "indices bits must be between 1 and 8, got {}",
                raw.bits
            ));
        }

        let expected = (COLUMN_SIZE * raw.bits as usize).div_ceil(8);
        if raw.buffer.len() != expected {
            return Err(format!(
                "indices buffer for {} bits must have {} bytes, got {}",
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

impl Indices {
    pub(crate) fn new(pallet_len: usize) -> Self {
        let bits = (pallet_len as f32).log2().ceil().max(1.0) as u8;
        let buffer_size = ((COLUMN_SIZE * bits as usize / 8usize) as f32).ceil() as usize;
        Self {
            bits,
            buffer: vec![0; buffer_size],
        }
    }

    fn get(&self, index: u8) -> u8 {
        //Convert the index to a bit-wise index;
        let bit = index as usize * self.bits as usize;
        //Get the byte index which would have the begining of our bits;
        let byte = bit / 8;
        //Get the position of the bit whitin the byte;
        let offset = bit % 8;

        //Since the bit can be spread across two bytes, we need to get a word (2 bytes)
        let lo = self.buffer[byte] as u16;
        let hi = self.buffer.get(byte + 1).copied().unwrap_or(0) as u16;
        let word = lo | (hi << 8);

        //Compute the mask where the bit is inside the word
        let mask = (1u16 << self.bits as u16) - 1;

        //Shift the word to the desired bit position and apply the mask, so only the relevant
        //bits are retrived
        ((word >> offset) & mask) as u8
    }

    fn set(&mut self, index: u8, value: u8) {
        //Convert the index to a bit-wise index;
        let bit = index as usize * self.bits as usize;
        //Get the byte index which would have the begining of our bits;
        let byte = bit / 8;
        //Get the position of the bit whitin the byte;
        let offset = bit % 8;

        //Since the bit can be spread across two bytes, we need to get a word (2 bytes)
        let lo = self.buffer[byte] as u16;
        let hi = self.buffer.get(byte + 1).copied().unwrap_or(0) as u16;
        let word = lo | (hi << 8);

        //Compute the mask where the bit is inside the word
        let mask = (1u16 << self.bits as u16) - 1;

        //Clear the existing value on the word
        let cleared = word & !(mask << offset);
        //Now apply the new value on the correct position
        let new_word = cleared | ((value as u16 & mask) << offset);

        self.buffer[byte] = new_word as u8;
        if byte + 1 < self.buffer.len() {
            self.buffer[byte + 1] = (new_word >> 8) as u8;
        }
    }

    fn iter(&self) -> IndicesIter<'_> {
        IndicesIter {
            indices: self,
            next: 0,
        }
    }

    fn grow(&mut self, new_size: u8) {
        let mut new_indices = Indices::new(new_size as usize);
        self.iter()
            .enumerate()
            .for_each(|(idx, value)| new_indices.set(idx as u8, value));

        *self = new_indices;
    }

    fn needs_to_grow(&self, new_size: u8) -> bool {
        new_size as u16 > 1 << self.bits as u16
    }
}

pub(crate) struct IndicesIter<'a> {
    indices: &'a Indices,
    next: usize,
}

impl<'a> Iterator for IndicesIter<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next < COLUMN_SIZE {
            let item = self.indices.get(self.next as u8);
            self.next += 1;
            Some(item)
        } else {
            None
        }
    }
}

#[allow(private_interfaces)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawChunkColumn<T>")]
pub(crate) enum ChunkColumn<T> {
    Single(T),
    Pallet {
        pallet: VoxelPallet<T>,
        indices: Indices,
    },
}

#[derive(Deserialize)]
enum RawChunkColumn<T> {
    Single(T),
    Pallet {
        pallet: VoxelPallet<T>,
        indices: Indices,
    },
}

impl<T> TryFrom<RawChunkColumn<T>> for ChunkColumn<T> {
    type Error = String;

    fn try_from(raw: RawChunkColumn<T>) -> Result<Self, Self::Error> {
        match raw {
            RawChunkColumn::Single(value) => Ok(ChunkColumn::Single(value)),
            RawChunkColumn::Pallet { pallet, indices } => {
                for index in indices.iter() {
                    if index as usize >= pallet.len() {
                        return Err(format!(
                            "voxel pallet index {} is out of bounds for {} entries",
                            index,
                            pallet.len()
                        ));
                    }
                }

                Ok(ChunkColumn::Pallet { pallet, indices })
            }
        }
    }
}

impl<T> Default for ChunkColumn<T>
where
    T: Default,
{
    fn default() -> Self {
        ChunkColumn::Single(Default::default())
    }
}

impl<T> ChunkColumn<T>
where
    T: ChunkStorageType,
{
    fn new_pallet() -> Self {
        Self::Pallet {
            pallet: VoxelPallet::default(),
            indices: Indices::new(0),
        }
    }

    #[inline]
    fn take(&mut self) -> ChunkColumn<T> {
        std::mem::take(self)
    }

    #[inline]
    fn replace(&mut self, mut new: ChunkColumn<T>) -> ChunkColumn<T> {
        std::mem::replace(self, new)
    }

    pub(crate) fn get(&self, index: u8) -> T {
        match &self {
            ChunkColumn::Single(value) => *value,
            ChunkColumn::Pallet { pallet, indices } => pallet[indices.get(index) as usize],
        }
    }

    pub(crate) fn set(&mut self, index: u8, value: T) {
        if let ChunkColumn::Single(current) = self
            && *current == value
        {
            // nothing to do here
            return;
        }

        let new_value = match self.take() {
            ChunkColumn::Single(voxel_value) => single_to_pallet(voxel_value, index, value),
            ChunkColumn::Pallet {
                mut pallet,
                mut indices,
            } => {
                if pallet.len() >= COLUMN_SIZE {
                    pallet_clean_up(&mut pallet, &mut indices, Some(index));
                }

                let pallet_index = pallet.find_or_add(value);

                if indices.needs_to_grow(pallet_index + 1) {
                    indices.grow(pallet_index + 1);
                }

                indices.set(index, pallet_index);

                ChunkColumn::Pallet { pallet, indices }
            }
        };
        self.replace(new_value);
    }

    pub(crate) fn pack(&mut self) {
        if matches!(self, ChunkColumn::Single(_)) {
            // nothing to do there
            return;
        }

        let new_value = match self.take() {
            ChunkColumn::Pallet {
                mut pallet,
                mut indices,
            } => {
                pallet_clean_up(&mut pallet, &mut indices, None);

                if pallet.len() == 1 {
                    ChunkColumn::Single(pallet[0])
                } else {
                    ChunkColumn::Pallet { pallet, indices }
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
            ChunkColumn::Single(v) => f(v),
            ChunkColumn::Pallet { pallet, indices } => {
                // If pallet isn't dirty, we can trust the pallet items is all used values.
                if !pallet.dirty {
                    pallet.iter().all(f)
                } else {
                    indices.iter().all(|idx| f(&pallet[idx as usize]))
                }
            }
        }
    }
}

fn single_to_pallet<T>(single: T, index: u8, new_value: T) -> ChunkColumn<T>
where
    T: ChunkStorageType,
{
    let mut pallet = VoxelPallet::new(vec![single, new_value]);
    pallet.dirty = true;

    // init indices point to existing voxel state on pallet
    let mut indices = Indices::new(pallet.len());

    // the new voxel state voxel and the second on the pallet
    indices.set(index, 1);

    ChunkColumn::Pallet { pallet, indices }
}

fn pallet_clean_up<T>(
    pallet: &mut VoxelPallet<T>,
    indices: &mut Indices,
    skip_index: Option<u8>,
) -> bool
where
    T: ChunkStorageType,
{
    if !pallet.dirty {
        return false;
    }

    let mut new_indices = [u8::MAX; COLUMN_SIZE];
    let mut new_pallet = VoxelPallet::empty();

    for i in 0..COLUMN_SIZE {
        if let Some(skip) = skip_index
            && i == skip as usize
        {
            continue;
        }

        let new_idx = new_pallet.find_or_add(pallet[indices.get(i as u8) as usize]);
        new_indices[i] = new_idx;
    }

    let res = if new_pallet.len() < pallet.len() {
        *indices = Indices::new(new_pallet.len());
        new_indices.iter().enumerate().for_each(|(idx, &value)| {
            indices.set(idx as u8, value);
        });
        *pallet = new_pallet;
        true
    } else {
        false
    };

    pallet.dirty = false;

    res
}

#[cfg(test)]
mod tests {
    use serde::{Serialize, de::DeserializeOwned};

    use super::*;

    fn encode<T: Serialize>(value: &T) -> Vec<u8> {
        bincode::serde::encode_to_vec(value, bincode::config::standard()).unwrap()
    }

    fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, bincode::error::DecodeError> {
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())
            .map(|(value, _)| value)
    }

    #[test]
    fn chunk_voxel_from_column_index() {
        // Arrange

        // Act
        let first = ChunkVoxel::from_column_index(0);
        let last = ChunkVoxel::from_column_index((COLUMN_COUNT - 1) as u8);

        // Assert
        assert_eq!(first, ChunkVoxel::new(0, 0, 0));
        assert_eq!(
            last,
            ChunkVoxel::new(
                (Chunk::X_AXIS_SIZE - 1) as u8,
                0,
                (Chunk::Z_AXIS_SIZE - 1) as u8
            )
        );
    }

    #[test]
    fn chunk_voxel_column_index() {
        // Arrange

        // Act
        let first = ChunkVoxel::new(0, 0, 0).column_index();
        let last = ChunkVoxel::new(
            (Chunk::X_AXIS_SIZE - 1) as u8,
            0,
            (Chunk::Z_AXIS_SIZE - 1) as u8,
        )
        .column_index();

        // Assert
        assert_eq!(first, 0);
        assert_eq!(last, COLUMN_COUNT - 1);
    }

    #[test]
    fn all() {
        // Arrange
        let mut single = ChunkColumn::Single(u16::MAX);
        let mut pallet = ChunkColumn::Single(u16::MAX);

        for i in 0..COLUMN_SIZE {
            single.set(i as u8, 123u16);
            pallet.set(i as u8, i as u16 % 100u16);
        }

        // Act

        // Assert
        assert!(single.all(|v| {
            assert_eq!(*v, 123u16);
            *v == 123u16
        }));
        assert!(pallet.all(|v| *v < u8::MAX as u16));
    }

    #[test]
    fn single_get() {
        // Arrange
        let state = 1u8;
        let single = ChunkColumn::Single(state);

        // Act
        let value = single.get(15);

        // Assert
        assert_eq!(value, state);
    }

    #[test]
    fn single_set_same() {
        // Arrange
        let state = 1u8;
        let mut single = ChunkColumn::Single(state);

        // Act
        single.set(15, state);

        // Assert
        assert!(matches!(single, ChunkColumn::Single(_)));
        assert_eq!(single.get(7), state);
    }

    #[test]
    fn single_set_diff() {
        // Arrange
        let state = 10u8;
        let diff_state = 2u8;
        let mut chunk = ChunkColumn::Single(state);

        // Act
        chunk.set(123, diff_state);

        // Assert
        assert!(matches!(chunk, ChunkColumn::Pallet { .. }));

        let new_state = chunk.get(123);
        assert_eq!(new_state, diff_state);

        let existing_state = chunk.get(15);
        assert_eq!(existing_state, state);
    }

    #[test]
    fn pallet_get_set_unique() {
        // Arrange
        let mut pallet = ChunkColumn::new_pallet();

        let indices = [123, 56, 70];

        let states = [1u8, 3u8, 4u8];

        // Act
        pallet.set(indices[0], states[0]);
        pallet.set(indices[1], states[1]);
        pallet.set(indices[2], states[2]);

        // Assert
        assert_eq!(pallet.get(indices[0]), states[0]);
        assert_eq!(pallet.get(indices[1]), states[1]);
        assert_eq!(pallet.get(indices[2]), states[2]);
    }

    #[test]
    fn pallet_get_set_non_unique() {
        // Arrange
        let mut pallet = ChunkColumn::new_pallet();

        let indices = [123, 56, 50, 66, 10];

        let states = [1u8, 2u8, 2u8, 4u8, 4u8];

        // Act
        for (idx, index) in indices.into_iter().enumerate() {
            pallet.set(index, states[idx]);
        }

        // Assert
        for (idx, index) in indices.into_iter().enumerate() {
            assert_eq!(pallet.get(index), states[idx]);
        }

        match pallet {
            // 3 new unique states + default state (air)
            ChunkColumn::Pallet { pallet, .. } => assert_eq!(pallet.len(), 4),
            _ => unreachable!("Pallet was changed somewhere"),
        }
    }

    #[test]
    fn pallet_get_set_no_overflow() {
        // Arrange
        let mut pallet = ChunkColumn::new_pallet();

        // Act
        for i in 0..u8::MAX {
            pallet.set(i, i);
        }

        // Assert
        for i in 0..u8::MAX {
            assert_eq!(pallet.get(i), i);
        }

        match pallet {
            ChunkColumn::Pallet { pallet, .. } => assert_eq!(pallet.len(), u8::MAX as usize),
            _ => unreachable!("Pallet was changed somewhere"),
        }
    }

    #[test]
    fn pallet_get_set_no_overflow_with_dead_state() {
        // Arrange
        let mut pallet = ChunkColumn::new_pallet();

        // Insert some dead states. They count towards the 255 entry limit.
        match &mut pallet {
            ChunkColumn::Pallet { pallet, .. } => {
                for i in 0u8..5 {
                    pallet.pallet.push(1000 + i as u16);
                }
            }
            _ => unreachable!(),
        }

        // Act: 5 dead states + the default state + 249 new states = 255 entries.
        for i in 0..250u16 {
            pallet.set(i as u8, i);
        }

        // Assert
        for i in 0..250u16 {
            assert_eq!(pallet.get(i as u8), i);
        }

        match pallet {
            ChunkColumn::Pallet { pallet, .. } => assert_eq!(pallet.len(), u8::MAX as usize),
            _ => unreachable!("Pallet was changed somewhere"),
        }
    }

    #[test]
    #[should_panic(expected = "cannot have more than 255 voxel types")]
    fn pallet_get_set_over_255_panics() {
        // Arrange
        let mut pallet = ChunkColumn::new_pallet();

        // Act: values 0..=255 are 256 distinct values.
        for i in 0..=u8::MAX {
            pallet.set(i, i as u16);
        }
    }

    #[test]
    fn pack_single() {
        // Arrange
        let state = 1u8;
        let mut single = ChunkColumn::Single(state);

        // Act
        single.pack();

        // Assert
        match single {
            ChunkColumn::Single(voxel_state) => assert_eq!(voxel_state, state),
            _ => panic!("Calling pack on single value should never change it"),
        }
    }

    #[test]
    fn pack_pallet_with_one_unique() {
        // Arrange
        let state = 1u8;

        let mut pallet = ChunkColumn::Pallet {
            pallet: VoxelPallet::new(vec![state]),
            indices: Indices::new(1),
        };

        // Act
        pallet.pack();

        // Assert
        match pallet {
            ChunkColumn::Single(voxel_state) => assert_eq!(voxel_state, state),
            _ => panic!("Calling pack on pallet with 1 unique value should return single"),
        }
    }

    #[test]
    fn pack_pallet_with_one_unique_with_dead_state() {
        // Arrange
        let state = 1u8;
        let dead = 2u8;

        let mut pallet = VoxelPallet::new(vec![state, dead]);
        pallet.dirty = true;

        let mut pallet = ChunkColumn::Pallet {
            pallet,
            indices: Indices::new(1),
        };

        // Act
        pallet.pack();

        // Assert
        match pallet {
            ChunkColumn::Single(voxel_state) => assert_eq!(voxel_state, state),
            _ => panic!("Calling pack on pallet with 1 unique value should return single"),
        }
    }

    #[test]
    fn column_storage_filter_single() {
        // Arrange
        let mut storage = ChunkColumnStorage::<u8>::new();
        chunk::voxels().for_each(|v| storage.set(v, 123u8));
        storage.pack();

        // Act
        let voxels = storage.filter(|value| *value == 123u8);

        // Assert
        assert_eq!(voxels.len(), Chunk::BUFFER_SIZE);
    }

    #[test]
    fn column_storage_filter_single_with_dead_states() {
        // Arrange
        let mut storage = ChunkColumnStorage::<u8>::new();
        chunk::voxels().for_each(|v| storage.set(v, 123u8));

        // Act
        let voxels = storage.filter(|value| *value == 123u8);

        // Assert
        assert_eq!(voxels.len(), Chunk::BUFFER_SIZE);
        assert!(voxels.iter().all(|v| storage.get(*v) == 123u8));
    }

    #[test]
    fn column_storage_filter_pallet() {
        // Arrange
        let mut storage = ChunkColumnStorage::<u8>::new();

        for x in 5..15 {
            for z in 3..10 {
                for y in 200..215 {
                    storage.set(ChunkVoxel::new(x, y, z), 123u8);
                }
            }
        }

        // Act
        let voxels = storage.filter(|value| *value == 123u8);

        // Assert
        assert_eq!(voxels.len(), 1050);
        assert!(voxels.iter().all(|v| storage.get(*v) == 123u8));
    }

    #[test]
    fn column_storage_filter_pallet_multiple() {
        // Arrange
        let mut storage = ChunkColumnStorage::<u8>::new();

        for x in 5..15 {
            for z in 3..10 {
                for y in 200..215 {
                    storage.set(ChunkVoxel::new(x, y, z), 123u8);
                }
            }
        }

        for x in 3..12 {
            for z in 7..11 {
                for y in 1..15 {
                    storage.set(ChunkVoxel::new(x, y, z), 33u8);
                }
            }
        }

        // Act
        let voxels_123 = storage.filter(|value| *value == 123u8);
        let voxels_33 = storage.filter(|value| *value == 33u8);
        let voxels_default = storage.filter(|value| *value == u8::default());

        // Assert
        assert_eq!(
            voxels_123.len() + voxels_33.len() + voxels_default.len(),
            Chunk::BUFFER_SIZE
        );
        assert!(voxels_123.iter().all(|v| storage.get(*v) == 123u8));
        assert!(voxels_33.iter().all(|v| storage.get(*v) == 33u8));
        assert!(
            voxels_default
                .iter()
                .all(|v| storage.get(*v) == u8::default())
        );
    }

    #[test]
    fn indices_serde_round_trip_all_bits() {
        for (pallet_len, expected_bits) in [
            (2usize, 1u8),
            (3, 2),
            (5, 3),
            (9, 4),
            (17, 5),
            (33, 6),
            (65, 7),
            (129, 8),
        ] {
            let mut indices = Indices::new(pallet_len);
            assert_eq!(indices.bits, expected_bits);

            let max_index = (pallet_len - 1) as u8;
            indices.set(0, max_index);

            let bytes = encode(&indices);
            let decoded: Indices = decode(&bytes).unwrap();

            assert_eq!(decoded, indices);
            assert_eq!(decoded.get(0), max_index);
            assert_eq!(decoded.get(1), 0);
        }
    }

    #[test]
    fn indices_serde_rejects_invalid_bits() {
        let invalid = Indices {
            bits: 0,
            buffer: Vec::new(),
        };
        assert!(decode::<Indices>(&encode(&invalid)).is_err());

        let invalid = Indices {
            bits: 9,
            buffer: vec![0; (COLUMN_SIZE * 9usize).div_ceil(8)],
        };
        assert!(decode::<Indices>(&encode(&invalid)).is_err());
    }

    #[test]
    fn indices_serde_rejects_wrong_buffer_len() {
        let invalid = Indices {
            bits: 2,
            buffer: vec![0; 10],
        };

        assert!(decode::<Indices>(&encode(&invalid)).is_err());
    }

    #[test]
    fn voxel_pallet_serde_rejects_too_many_entries() {
        let invalid = VoxelPallet {
            pallet: vec![0u16; u8::MAX as usize + 1],
            dirty: false,
        };

        assert!(decode::<VoxelPallet<u16>>(&encode(&invalid)).is_err());
    }

    #[test]
    fn voxel_pallet_serde_accepts_255_entries() {
        let valid = VoxelPallet {
            pallet: vec![0u16; u8::MAX as usize],
            dirty: false,
        };

        let decoded: VoxelPallet<u16> = decode(&encode(&valid)).unwrap();
        assert_eq!(decoded.pallet.len(), u8::MAX as usize);
    }

    #[test]
    fn chunk_column_serde_rejects_out_of_bounds_index() {
        let mut indices = Indices::new(3);
        indices.set(0, 3);

        let invalid = ChunkColumn::Pallet {
            pallet: VoxelPallet::new(vec![1u16, 2, 3]),
            indices,
        };

        assert!(decode::<ChunkColumn<u16>>(&encode(&invalid)).is_err());
    }

    #[test]
    fn chunk_column_serde_round_trip_pallet() {
        let mut column = ChunkColumn::new_pallet();
        column.set(0, 1u16);
        column.set(1, 2u16);
        column.set(2, 1u16);

        let decoded: ChunkColumn<u16> = decode(&encode(&column)).unwrap();

        assert_eq!(decoded.get(0), 1);
        assert_eq!(decoded.get(1), 2);
        assert_eq!(decoded.get(2), 1);
    }
}
