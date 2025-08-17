use bevy::math::{IVec2, IVec3, Vec3};
use serde::{Deserialize, Serialize};

use crate::coords::Voxel;

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

    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub fn neighbor(self, dir: IVec2) -> Self {
        Chunk::from(IVec2::from(self) + dir)
    }
}

impl std::fmt::Display for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("({}, {})", self.x, self.z))
    }
}

impl From<Vec3> for Chunk {
    fn from(world: Vec3) -> Self {
        let x = world.x.floor() as i32;
        let z = world.z.floor() as i32;
        Self { x, z }
    }
}

impl From<Chunk> for Vec3 {
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

#[derive(Debug, Default, PartialEq, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct ChunkVoxel {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl ChunkVoxel {
    pub const fn new(x: u8, y: u8, z: u8) -> Self {
        Self { x, y, z }
    }

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
}

impl std::fmt::Display for ChunkVoxel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("({}, {}, {})", self.x, self.y, self.z))
    }
}

impl From<ChunkVoxel> for usize {
    fn from(value: ChunkVoxel) -> usize {
        (value.x as usize) << Chunk::X_SHIFT
            | (value.y as usize) << Chunk::Y_SHIFT
            | (value.z as usize) << Chunk::Z_SHIFT
    }
}

impl From<usize> for ChunkVoxel {
    fn from(index: usize) -> Self {
        ChunkVoxel::new(
            ((index & Chunk::X_MASK) >> Chunk::X_SHIFT) as u8,
            ((index & Chunk::Y_MASK) >> Chunk::Y_SHIFT) as u8,
            ((index & Chunk::Z_MASK) >> Chunk::Z_SHIFT) as u8,
        )
    }
}

impl From<Vec3> for ChunkVoxel {
    fn from(world: Vec3) -> Self {
        // First round world coords to integer.
        // This transform (1.1, -0.3, 17.5) into (1, -1, 17)
        let voxel = Voxel::from(world);

        // Get the euclidean remainder
        // This transform (1, -1, 17) into (1, 15, 1)
        let x = voxel.x.rem_euclid(Chunk::X_AXIS_SIZE as i32) as u8;
        let y = voxel.y.rem_euclid(Chunk::Y_AXIS_SIZE as i32) as u8;
        let z = voxel.z.rem_euclid(Chunk::Z_AXIS_SIZE as i32) as u8;

        Self { x, y, z }
    }
}

impl From<ChunkVoxel> for IVec2 {
    fn from(value: ChunkVoxel) -> Self {
        IVec2::new(value.x as i32, value.z as i32)
    }
}

impl From<ChunkVoxel> for IVec3 {
    fn from(value: ChunkVoxel) -> Self {
        IVec3::new(value.x as i32, value.y as i32, value.z as i32)
    }
}

impl From<(u8, u8, u8)> for ChunkVoxel {
    fn from(value: (u8, u8, u8)) -> Self {
        Self {
            x: value.0,
            y: value.1,
            z: value.2,
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub(crate) struct ColumnVoxel {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl ColumnVoxel {
    pub fn new(x: u8, y: u8, z: u8) -> Self {
        Self {
            x: x & 0x0F,
            y,
            z: z & 0x0F,
        }
    }

    pub fn from_index(index: u8) -> Self {
        ColumnVoxel::new((index & 0xF0) >> 4, 0, index & 0x0F)
    }

    pub fn column_index(&self) -> usize {
        (self.x << 4 | self.z) as usize
    }
}

impl From<ChunkVoxel> for ColumnVoxel {
    fn from(value: ChunkVoxel) -> Self {
        Self::new(value.x, value.y, value.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_voxel_from_index() {
        assert_eq!(ChunkVoxel::new(0, 0, 0), 0usize.into());
        assert_eq!(ChunkVoxel::new(0, 1, 0), 1usize.into());
        assert_eq!(ChunkVoxel::new(0, 2, 0), 2usize.into());

        assert_eq!(
            ChunkVoxel::new(0, 0, 1),
            Chunk::Y_AXIS_SIZE.into(),
            "X >> Z >> Y, so one Z unit should be a full Y axis"
        );
        assert_eq!(ChunkVoxel::new(0, 1, 1), (Chunk::Y_AXIS_SIZE + 1).into());
        assert_eq!(ChunkVoxel::new(0, 2, 1), (Chunk::Y_AXIS_SIZE + 2).into());

        assert_eq!(
            ChunkVoxel::new(1, 0, 0),
            (Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE).into()
        );
        assert_eq!(
            ChunkVoxel::new(1, 1, 0),
            (Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + 1).into()
        );
        assert_eq!(
            ChunkVoxel::new(1, 2, 0),
            (Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + 2).into()
        );

        assert_eq!(
            ChunkVoxel::new(1, 0, 1),
            (Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE).into()
        );
        assert_eq!(
            ChunkVoxel::new(1, 1, 1),
            (Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE + 1).into()
        );
        assert_eq!(
            ChunkVoxel::new(1, 2, 1),
            (Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE + 2).into()
        );
    }

    #[test]
    fn chunk_voxel_to_index() {
        assert_eq!(usize::from(ChunkVoxel::new(0, 0, 0)), 0usize);
        assert_eq!(usize::from(ChunkVoxel::new(0, 1, 0)), 1usize);
        assert_eq!(usize::from(ChunkVoxel::new(0, 2, 0)), 2usize);

        assert_eq!(usize::from(ChunkVoxel::new(0, 0, 1)), Chunk::Y_AXIS_SIZE);
        assert_eq!(
            usize::from(ChunkVoxel::new(0, 1, 1)),
            Chunk::Y_AXIS_SIZE + 1
        );
        assert_eq!(
            usize::from(ChunkVoxel::new(0, 2, 1)),
            Chunk::Y_AXIS_SIZE + 2
        );

        assert_eq!(
            usize::from(ChunkVoxel::new(1, 0, 0)),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE
        );
        assert_eq!(
            usize::from(ChunkVoxel::new(1, 1, 0)),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + 1
        );
        assert_eq!(
            usize::from(ChunkVoxel::new(1, 2, 0)),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + 2
        );

        assert_eq!(
            usize::from(ChunkVoxel::new(1, 0, 1)),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE
        );
        assert_eq!(
            usize::from(ChunkVoxel::new(1, 1, 1)),
            Chunk::Y_AXIS_SIZE * Chunk::Z_AXIS_SIZE + Chunk::Y_AXIS_SIZE + 1
        );
        assert_eq!(
            usize::from(ChunkVoxel::new(1, 2, 1)),
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
