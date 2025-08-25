use bevy::math::{IVec2, IVec3, Vec3};
use serde::{Deserialize, Serialize};

use crate::coords::Voxel;

/// Points to a chunk coordinates in the world in a 2d grid.
///
/// A Chunk is a 3d grid container with [`Self::BUFFER_SIZE`] [`Voxel`]s.
#[derive(Debug, Default, Hash, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
}

impl Chunk {
    pub const X_AXIS_SIZE: usize = 16;
    pub const Y_AXIS_SIZE: usize = 256;
    pub const Z_AXIS_SIZE: usize = 16;
    pub const X_END: u8 = (Self::X_AXIS_SIZE - 1) as u8;
    pub const Y_END: u8 = (Self::Y_AXIS_SIZE - 1) as u8;
    pub const Z_END: u8 = (Self::Z_AXIS_SIZE - 1) as u8;

    pub const BUFFER_SIZE: usize = Self::X_AXIS_SIZE * Self::Z_AXIS_SIZE * Self::Y_AXIS_SIZE;

    const X_SHIFT: usize = (Self::Z_AXIS_SIZE.ilog2() + Self::Z_SHIFT as u32) as usize;
    const Z_SHIFT: usize = Self::Y_AXIS_SIZE.ilog2() as usize;
    const Y_SHIFT: usize = 0;

    const X_MASK: usize = (Self::X_AXIS_SIZE - 1) << Self::X_SHIFT;
    const Z_MASK: usize = (Self::Z_AXIS_SIZE - 1) << Self::Z_SHIFT;
    const Y_MASK: usize = Self::Y_AXIS_SIZE - 1;

    /// Crates a new chunk coordinates.
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Creates a new chunk coordinates pointing to a neighbor chunk at the given direction.
    pub fn neighbor(self, dir: IVec2) -> Self {
        Chunk {
            x: self.x + dir.x,
            z: self.z + dir.y,
        }
    }
}

impl std::fmt::Display for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("({}, {})", self.x, self.z))
    }
}

impl From<Vec3> for Chunk {
    /// Converts a point in the world to a chunk coordinate.
    ///
    /// This handles negative coordinates, so (-1.2, -0.3) will point to (-1, 0).
    fn from(world: Vec3) -> Self {
        let x = world.x.floor() as i32;
        let z = world.z.floor() as i32;
        Self { x, z }
    }
}

impl From<Chunk> for Vec3 {
    /// Converts a chunk coordinate to a global world position.
    ///
    /// This will return the absolute position, so (1, 3) will point to (1 *
    /// [`Chunk::X_AXIS_SIZE`], 3 * [`Chunk::Z_AXIS_SIZE`])
    fn from(chunk: Chunk) -> Self {
        Self {
            x: chunk.x as f32 * Chunk::X_AXIS_SIZE as f32,
            y: 0.0,
            z: chunk.z as f32 * Chunk::Z_AXIS_SIZE as f32,
        }
    }
}

impl From<(i32, i32)> for Chunk {
    fn from(value: (i32, i32)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl From<IVec2> for Chunk {
    fn from(value: IVec2) -> Self {
        Self::new(value.x, value.y)
    }
}

impl From<Chunk> for IVec2 {
    fn from(value: Chunk) -> Self {
        Self::new(value.x, value.z)
    }
}

/// Represents a voxel coordinate inside a [`Chunk`]. Since it is a relative coordinate, it can't
/// be negative and is guaranteed to be within chunk bounds.
#[derive(Debug, Default, PartialEq, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct ChunkVoxel {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl ChunkVoxel {
    #[inline(always)]
    pub const fn new(x: u8, y: u8, z: u8) -> Self {
        Self { x, y, z }
    }

    #[inline]
    /// Try to convert to a valid chunk voxel coordinates. If the given coordinates is outside the
    /// chunk bounds, returns [`None`].
    pub fn try_from(value: IVec3) -> Option<Self> {
        if value.x >= 0
            && value.x < Chunk::X_AXIS_SIZE as i32
            && value.y >= 0
            && value.y < Chunk::Y_AXIS_SIZE as i32
            && value.z >= 0
            && value.z < Chunk::Z_AXIS_SIZE as i32
        {
            Some(Self::new(value.x as u8, value.y as u8, value.z as u8))
        } else {
            None
        }
    }

    #[inline(always)]
    /// Converts this 3d coordinates into a 1d coordinate
    pub const fn to_index(self: ChunkVoxel) -> usize {
        (self.x as usize) << Chunk::X_SHIFT
            | (self.y as usize) << Chunk::Y_SHIFT
            | (self.z as usize) << Chunk::Z_SHIFT
    }

    #[inline(always)]
    /// Creates a new 3d coordinates from a 1d coordinate
    pub const fn from_index(index: usize) -> ChunkVoxel {
        ChunkVoxel::new(
            ((index & Chunk::X_MASK) >> Chunk::X_SHIFT) as u8,
            ((index & Chunk::Y_MASK) >> Chunk::Y_SHIFT) as u8,
            ((index & Chunk::Z_MASK) >> Chunk::Z_SHIFT) as u8,
        )
    }

    /// Crates a new chunk voxel coordinates from a world coordinates.
    ///
    /// This converts (1.1, -0.3, 17.5) into (1, 15,1), since given the world coordinates,
    /// that's where this voxel would be inside a chunk at this position.
    #[inline(always)]
    pub fn from_world(world: Vec3) -> Self {
        // First round world coords to integer.
        // This transform (1.1, -0.3, 17.5) into (1, -1, 17)
        let voxel = Voxel::from_world(world);

        // Get the euclidean remainder
        // This transform (1, -1, 17) into (1, 15, 1)
        let x = voxel.x.rem_euclid(Chunk::X_AXIS_SIZE as i32) as u8;
        let y = voxel.y.rem_euclid(Chunk::Y_AXIS_SIZE as i32) as u8;
        let z = voxel.z.rem_euclid(Chunk::Z_AXIS_SIZE as i32) as u8;

        Self::new(x, y, z)
    }
}

impl std::fmt::Display for ChunkVoxel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("({}, {}, {})", self.x, self.y, self.z))
    }
}

impl From<ChunkVoxel> for IVec2 {
    fn from(value: ChunkVoxel) -> Self {
        IVec2::new(value.x as i32, value.z as i32)
    }
}

impl From<ChunkVoxel> for IVec3 {
    #[inline]
    fn from(value: ChunkVoxel) -> Self {
        IVec3::new(value.x as i32, value.y as i32, value.z as i32)
    }
}

impl From<(u8, u8, u8)> for ChunkVoxel {
    fn from(value: (u8, u8, u8)) -> Self {
        Self::new(value.0, value.1, value.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_voxel_from_index() {
        assert_eq!(ChunkVoxel::new(0, 0, 0), ChunkVoxel::from_index(0));
        assert_eq!(ChunkVoxel::new(0, 1, 0), ChunkVoxel::from_index(1));
        assert_eq!(ChunkVoxel::new(0, 2, 0), ChunkVoxel::from_index(2));

        assert_eq!(
            ChunkVoxel::new(0, 0, 1),
            ChunkVoxel::from_index(Chunk::Y_AXIS_SIZE),
            "X >> Z >> Y, so one Z unit should be a full Y axis"
        );
        assert_eq!(
            ChunkVoxel::new(0, 1, 1),
            ChunkVoxel::from_index(Chunk::Y_AXIS_SIZE + 1)
        );
        assert_eq!(
            ChunkVoxel::new(0, 2, 1),
            ChunkVoxel::from_index(Chunk::Y_AXIS_SIZE + 2)
        );

        assert_eq!(
            ChunkVoxel::new(1, 0, 0),
            ChunkVoxel::from_index(Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE)
        );
        assert_eq!(
            ChunkVoxel::new(1, 1, 0),
            ChunkVoxel::from_index(Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + 1)
        );
        assert_eq!(
            ChunkVoxel::new(1, 2, 0),
            ChunkVoxel::from_index(Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + 2)
        );

        assert_eq!(
            ChunkVoxel::new(1, 0, 1),
            ChunkVoxel::from_index(Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE)
        );
        assert_eq!(
            ChunkVoxel::new(1, 1, 1),
            ChunkVoxel::from_index(
                Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE + 1
            )
        );
        assert_eq!(
            ChunkVoxel::new(1, 2, 1),
            ChunkVoxel::from_index(
                Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE + 2
            )
        );
    }

    #[test]
    fn chunk_voxel_to_index() {
        assert_eq!(ChunkVoxel::new(0, 0, 0).to_index(), 0usize);
        assert_eq!(ChunkVoxel::new(0, 1, 0).to_index(), 1usize);
        assert_eq!(ChunkVoxel::new(0, 2, 0).to_index(), 2usize);

        assert_eq!(ChunkVoxel::new(0, 0, 1).to_index(), Chunk::Y_AXIS_SIZE);
        assert_eq!(ChunkVoxel::new(0, 1, 1).to_index(), Chunk::Y_AXIS_SIZE + 1);
        assert_eq!(ChunkVoxel::new(0, 2, 1).to_index(), Chunk::Y_AXIS_SIZE + 2);

        assert_eq!(
            ChunkVoxel::new(1, 0, 0).to_index(),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE
        );
        assert_eq!(
            ChunkVoxel::new(1, 1, 0).to_index(),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + 1
        );
        assert_eq!(
            ChunkVoxel::new(1, 2, 0).to_index(),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + 2
        );

        assert_eq!(
            ChunkVoxel::new(1, 0, 1).to_index(),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE
        );
        assert_eq!(
            ChunkVoxel::new(1, 1, 1).to_index(),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE + 1
        );
        assert_eq!(
            ChunkVoxel::new(1, 2, 1).to_index(),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE + 2
        );
    }

    #[test]
    fn chunk_to_world() {
        use super::*;

        assert_eq!(
            Vec3::from(Chunk::new(0, -2)),
            Vec3::new(0.0, 0.0, Chunk::Z_AXIS_SIZE as f32 * -2.0)
        );
        assert_eq!(
            Vec3::from(Chunk::new(3, 1)),
            Vec3::new(
                Chunk::X_AXIS_SIZE as f32 * 3.0,
                0.0,
                Chunk::Z_AXIS_SIZE as f32 * 1.0
            )
        );
    }
}
