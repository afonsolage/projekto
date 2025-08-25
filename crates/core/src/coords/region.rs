use crate::coords::Chunk;

/// Points to a region coordinates in the world in a 2d grid.
///
/// A Region is a 2d grid container with [`Self::BUFFER_SIZE`] [`Chunk`]s.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct Region {
    pub x: i32,
    pub z: i32,
}

impl Region {
    pub const AXIS_SIZE: usize = 32;
    pub const X_END: u8 = (Self::AXIS_SIZE - 1) as u8;
    pub const Z_END: u8 = (Self::AXIS_SIZE - 1) as u8;

    pub const BUFFER_SIZE: usize = Self::AXIS_SIZE * Self::AXIS_SIZE;

    const X_SHIFT: usize = Self::AXIS_SIZE.ilog2() as usize;
    // const Z_SHIFT: usize = 0;
    //
    // const X_MASK: usize = (Self::AXIS_SIZE - 1) << Self::X_SHIFT;
    // const Z_MASK: usize = (Self::AXIS_SIZE - 1) << Self::Z_SHIFT;

    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub fn from_chunk(chunk: Chunk) -> Self {
        Region::new(
            ((chunk.x as f32) / Region::AXIS_SIZE as f32).floor() as i32,
            ((chunk.z as f32) / Region::AXIS_SIZE as f32).floor() as i32,
        )
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("({}, {})", self.x, self.z))
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct RegionChunk {
    pub x: u8,
    pub z: u8,
}

impl RegionChunk {
    #[inline(always)]
    pub const fn new(x: u8, z: u8) -> Self {
        Self { x, z }
    }

    #[inline(always)]
    pub const fn to_index(self) -> usize {
        (self.x as usize) << Region::X_SHIFT | self.z as usize
    }

    pub fn from_chunk(chunk: Chunk) -> Self {
        let x = chunk.x.rem_euclid(Region::AXIS_SIZE as i32);
        let z = chunk.z.rem_euclid(Region::AXIS_SIZE as i32);

        debug_assert!(x >= 0 && x <= Region::AXIS_SIZE as i32);
        debug_assert!(z >= 0 && z <= Region::AXIS_SIZE as i32);

        Self::new(x as u8, z as u8)
    }
}

impl std::fmt::Display for RegionChunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("({}, {})", self.x, self.z))
    }
}
