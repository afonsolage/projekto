#![allow(unused)]
use std::ops::{Deref, DerefMut};

use bevy::{
    math::{IVec2, Vec3, Vec3Swizzles},
    platform::collections::HashSet,
    prelude::{Deref, DerefMut},
};
use serde::{Deserialize, Serialize};

use crate::{
    chunk::{self, ChunkStorageType},
    voxel::{self, FacesOcclusion, FacesSoftLight, Kind, Light, LightTy, Voxel},
};

const COLUMN_SIZE: usize = super::Y_AXIS_SIZE;
const COLUMN_COUNT: usize = chunk::X_AXIS_SIZE * chunk::Z_AXIS_SIZE;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ChunkColumnStorage<T>(Vec<ChunkColumn<T>>);

impl<T> ChunkColumnStorage<T> {
    #[inline]
    fn to_column_index(voxel: Voxel) -> usize {
        assert!(voxel.x >= 0 && voxel.x <= 15);
        assert!(voxel.z >= 0 && voxel.z <= 15);

        (voxel.x << 4 | voxel.z) as usize
    }
}

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

impl<T> ChunkColumnStorage<T>
where
    T: ChunkStorageType,
{
    pub fn get(&self, voxel: Voxel) -> T {
        let column_index = Self::to_column_index(voxel);

        assert!(voxel.y <= u8::MAX as i32);

        self.0[column_index].get(voxel.y as u8)
    }

    pub fn set(&mut self, voxel: Voxel, value: T) {
        let column_index = Self::to_column_index(voxel);

        assert!(voxel.y <= u8::MAX as i32);

        self.0[column_index].set(voxel.y as u8, value);
    }

    pub fn is_default(&self) -> bool {
        self.0
            .iter()
            .all(|pack| matches!(pack, ChunkColumn::Single(_)))
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
struct VoxelPallet<T> {
    pallet: Vec<T>,
    dirty: bool,
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

    fn find_or_add(&mut self, value: T) -> u8 {
        if let Some(index) = self.pallet.iter().position(|s| *s == value) {
            assert!(index <= u8::MAX as usize);

            index as u8
        } else {
            self.pallet.push(value);
            self.dirty = true;

            let new_index = self.pallet.len() - 1;

            assert!(
                new_index <= u8::MAX as usize,
                "new_index ({new_index}) overflow u8 max"
            );

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

#[derive(Clone, Debug, PartialEq, Deref, DerefMut, Serialize, Deserialize)]
#[repr(transparent)]
pub(crate) struct Indices(pub(crate) Vec<u8>);

#[allow(private_interfaces)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum ChunkColumn<T> {
    Single(T),
    Pallet {
        pallet: VoxelPallet<T>,
        indices: Indices,
    },
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
            indices: Indices(vec![0; COLUMN_SIZE]),
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
            ChunkColumn::Pallet { pallet, indices } => pallet[indices[index as usize] as usize],
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
                indices[index as usize] = pallet_index;
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
                    indices.0.iter().all(|idx| f(&pallet[*idx as usize]))
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
    let mut indices = Indices(vec![0; COLUMN_SIZE]);

    // the new voxel state voxel and the second on the pallet
    indices[index as usize] = 1;

    ChunkColumn::Pallet { pallet, indices }
}

fn pallet_clean_up<T>(
    pallet: &mut VoxelPallet<T>,
    indices: &mut [u8],
    skip_index: Option<u8>,
) -> bool
where
    T: ChunkStorageType,
{
    if !pallet.dirty {
        return false;
    }

    pallet.dirty = false;

    let mut new_indices = [u8::MAX; COLUMN_SIZE];
    let mut new_pallet = VoxelPallet::empty();

    for i in 0..COLUMN_SIZE {
        if let Some(skip) = skip_index
            && i == skip as usize
        {
            continue;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

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

        // insert some dead states
        match &mut pallet {
            ChunkColumn::Pallet { pallet, indices } => {
                for i in 0u8..20 {
                    pallet.pallet.push(1000 + i as u16);
                }
            }
            _ => unreachable!(),
        }

        // Act
        for i in 0..u8::MAX {
            pallet.set(i, i as u16);
        }

        // Assert
        for i in 0..u8::MAX {
            assert_eq!(pallet.get(i), i as u16);
        }

        match pallet {
            ChunkColumn::Pallet { pallet, .. } => assert_eq!(pallet.len(), u8::MAX as usize),
            _ => unreachable!("Pallet was changed somewhere"),
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
            indices: Indices(vec![0; COLUMN_SIZE]),
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
            indices: Indices(vec![0; COLUMN_SIZE]),
        };

        // Act
        pallet.pack();

        // Assert
        match pallet {
            ChunkColumn::Single(voxel_state) => assert_eq!(voxel_state, state),
            _ => panic!("Calling pack on pallet with 1 unique value should return single"),
        }
    }
}
