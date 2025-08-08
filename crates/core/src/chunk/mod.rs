use crate::{
    math,
    voxel::{self, Voxel},
};
use bevy::math::{IVec2, IVec3, Vec3};
use serde::{Deserialize, Serialize};

mod storage;
pub use storage::*;

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

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chunk(IVec2);

impl Chunk {
    pub fn new(x: i32, z: i32) -> Self {
        Self(IVec2::new(x, z))
    }

    pub fn from_path(path: &std::path::Path) -> Self {
        let file_name = path
            .file_name()
            .expect("To be a valid chunk path")
            .to_str()
            .expect("To be UTF-8 string");
        let (x, z) = file_name
            .split_once('_')
            .expect("Chunk path must be composed of X_Z");
        Self(IVec2::new(
            x.parse().expect("X on chunk path to be a valid i32"),
            z.parse().expect("Z on chunk path to be a valid i32"),
        ))
    }

    pub fn z(&self) -> i32 {
        self.0.y
    }

    pub fn x(&self) -> i32 {
        self.0.x
    }

    pub fn xz(&self) -> IVec2 {
        self.0
    }

    pub fn neighbor(&self, dir: IVec2) -> Self {
        Chunk(self.0 + dir)
    }

    pub fn distance(&self, other: Chunk) -> IVec2 {
        other.0 - self.0
    }

    pub fn path(&self) -> String {
        format!("chunk://{}_{}", self.0.x, self.0.y)
    }
}

impl From<IVec2> for Chunk {
    fn from(value: IVec2) -> Self {
        Self(value)
    }
}

impl From<Chunk> for IVec2 {
    fn from(value: Chunk) -> Self {
        value.0
    }
}

impl From<(i32, i32)> for Chunk {
    fn from(value: (i32, i32)) -> Self {
        Self(value.into())
    }
}

impl From<Vec3> for Chunk {
    fn from(value: Vec3) -> Self {
        to_chunk(value)
    }
}

impl From<Chunk> for Vec3 {
    fn from(value: Chunk) -> Self {
        to_world(value)
    }
}

impl std::fmt::Display for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub const SIDE_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash, Default, Serialize, Deserialize)]
pub enum ChunkSide {
    #[default]
    Right = 0,
    Left = 1,
    Front = 2,
    Back = 3,
}

pub const SIDES: [ChunkSide; SIDE_COUNT] = [
    ChunkSide::Right,
    ChunkSide::Left,
    ChunkSide::Front,
    ChunkSide::Back,
];

impl ChunkSide {
    // pub const fn opposite(&self) -> ChunkSide {
    //     match self {
    //         ChunkSide::Right => ChunkSide::Left,
    //         ChunkSide::Left => ChunkSide::Right,
    //         ChunkSide::Front => ChunkSide::Back,
    //         ChunkSide::Back => ChunkSide::Front,
    //     }
    // }

    pub const fn index(&self) -> usize {
        match self {
            ChunkSide::Right => 0,
            ChunkSide::Left => 1,
            ChunkSide::Front => 2,
            ChunkSide::Back => 3,
        }
    }

    pub const fn dir(&self) -> IVec2 {
        match self {
            ChunkSide::Right => IVec2::X,
            ChunkSide::Left => IVec2::NEG_X,
            ChunkSide::Front => IVec2::Y,
            ChunkSide::Back => IVec2::NEG_Y,
        }
    }

    // pub const fn from_dir(dir: IVec2) -> ChunkSide {
    //     if dir.x == 1 {
    //         ChunkSide::Right
    //     } else if dir.x == -1 {
    //         ChunkSide::Left
    //     } else if dir.y == 1 {
    //         ChunkSide::Front
    //     } else if dir.y == -1 {
    //         ChunkSide::Back
    //     } else {
    //         panic!("Invalid direction received")
    //     }
    // }

    pub const fn from_voxel_side(side: voxel::Side) -> Option<ChunkSide> {
        match side {
            voxel::Side::Right => Some(Self::Right),
            voxel::Side::Left => Some(Self::Left),
            voxel::Side::Front => Some(Self::Front),
            voxel::Side::Back => Some(Self::Back),
            _ => None,
        }
    }
}

#[inline]
pub fn to_index(voxel: Voxel) -> usize {
    (voxel.x << X_SHIFT | voxel.y << Y_SHIFT | voxel.z << Z_SHIFT) as usize
}

#[inline]
pub fn from_index(index: usize) -> Voxel {
    Voxel::new(
        ((index & X_MASK) >> X_SHIFT) as i32,
        ((index & Y_MASK) >> Y_SHIFT) as i32,
        ((index & Z_MASK) >> Z_SHIFT) as i32,
    )
}

pub fn voxels() -> impl Iterator<Item = Voxel> {
    (0..BUFFER_SIZE).map(from_index)
}

pub fn top_voxels() -> impl Iterator<Item = Voxel> {
    (0..=X_END).flat_map(|x| (0..=Z_END).map(move |z| Voxel::new(x, Y_END, z)))
}

#[inline]
pub fn is_inside(voxel: Voxel) -> bool {
    voxel.x >= 0
        && voxel.x < X_AXIS_SIZE as i32
        && voxel.z >= 0
        && voxel.z < Z_AXIS_SIZE as i32
        && voxel.y >= 0
        && voxel.y < Y_AXIS_SIZE as i32
}

pub fn is_at_edge(voxel: Voxel) -> bool {
    voxel.x == 0
        || voxel.y == 0
        || voxel.z == 0
        || voxel.x == (X_AXIS_SIZE - 1) as i32
        || voxel.y == (Y_AXIS_SIZE - 1) as i32
        || voxel.z == (Z_AXIS_SIZE - 1) as i32
}

pub fn to_world(chunk: Chunk) -> Vec3 {
    Vec3::new(
        X_AXIS_SIZE as f32 * chunk.0.x as f32,
        0.0,
        Z_AXIS_SIZE as f32 * chunk.0.y as f32,
    )
}

pub fn to_chunk(world: Vec3) -> Chunk {
    Chunk::new(
        (world.x / X_AXIS_SIZE as f32).floor() as i32,
        (world.z / Z_AXIS_SIZE as f32).floor() as i32,
    )
}

pub fn overlap_voxel(voxel: Voxel) -> (IVec2, Voxel) {
    debug_assert!(!is_inside(voxel), "Voxel {voxel} does't overlap");
    debug_assert!(
        voxel.y >= 0 && voxel.y < Y_AXIS_SIZE as i32,
        "Can't overlap up or down. There is never chunk above or bellow"
    );

    let overlapping_voxel = math::euclid_rem(
        voxel,
        IVec3::new(X_AXIS_SIZE as i32, Y_AXIS_SIZE as i32, Z_AXIS_SIZE as i32),
    );

    let overlapping_dir = (
        if voxel.x < 0 {
            -1
        } else if voxel.x >= X_AXIS_SIZE as i32 {
            1
        } else {
            0
        },
        if voxel.z < 0 {
            -1
        } else if voxel.z >= Z_AXIS_SIZE as i32 {
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
    use rand::Rng;

    use super::*;

    #[test]
    fn from_index() {
        assert_eq!(Voxel::new(0, 0, 0), super::from_index(0));
        assert_eq!(Voxel::new(0, 1, 0), super::from_index(1));
        assert_eq!(Voxel::new(0, 2, 0), super::from_index(2));

        assert_eq!(
            Voxel::new(0, 0, 1),
            super::from_index(super::Y_AXIS_SIZE),
            "X >> Z >> Y, so one Z unit should be a full Y axis"
        );
        assert_eq!(
            Voxel::new(0, 1, 1),
            super::from_index(super::Y_AXIS_SIZE + 1)
        );
        assert_eq!(
            Voxel::new(0, 2, 1),
            super::from_index(super::Y_AXIS_SIZE + 2)
        );

        assert_eq!(
            Voxel::new(1, 0, 0),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE)
        );
        assert_eq!(
            Voxel::new(1, 1, 0),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + 1)
        );
        assert_eq!(
            Voxel::new(1, 2, 0),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + 2)
        );

        assert_eq!(
            Voxel::new(1, 0, 1),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + super::Y_AXIS_SIZE)
        );
        assert_eq!(
            Voxel::new(1, 1, 1),
            super::from_index(super::Y_AXIS_SIZE * super::Z_AXIS_SIZE + super::Y_AXIS_SIZE + 1)
        );
        assert_eq!(
            Voxel::new(1, 2, 1),
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
    fn to_world() {
        use super::*;

        assert_eq!(
            super::to_world(Chunk::new(0, -2)),
            Vec3::new(0.0, 0.0, super::Z_AXIS_SIZE as f32 * -2.0)
        );
        assert_eq!(
            super::to_world(Chunk::new(3, 1)),
            Vec3::new(
                super::X_AXIS_SIZE as f32 * 3.0,
                0.0,
                super::Z_AXIS_SIZE as f32 * 1.0
            )
        );
    }

    // #[test]
    // fn to_chunk() {
    //     use super::*;
    //
    //     assert_eq!(
    //         Chunk::new(0, -2),
    //         super::to_chunk(Vec3::new(0.0, 0.0, -2.0))
    //     );
    //     assert_eq!(Chunk::new(0, 0), super::to_chunk(Vec3::new(0.0, 0.0, 0.0)));
    // }

    #[test]
    fn voxels() {
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
    fn top_voxels() {
        let top_voxels = super::top_voxels().collect::<Vec<_>>();

        assert_eq!(top_voxels.len(), X_AXIS_SIZE * Z_AXIS_SIZE);
        top_voxels
            .into_iter()
            .for_each(|voxel| assert_eq!(voxel.y, Y_END));
    }

    #[test]
    fn set_get() {
        let mut chunk = ChunkStorage::<u8>::default();

        let mut rnd = rand::rng();
        for v in super::voxels() {
            let k = rnd.random::<u8>();
            chunk.set(v, k);
            assert_eq!(k, chunk.get(v));
        }
    }

    // #[test]
    // fn is_default() {
    //     impl ChunkStorageType for [u8; 3] {}
    //
    //     let mut chunk = ChunkStorage::<[u8; 3]>::default();
    //     assert!(chunk.is_default());
    //
    //     chunk.set((1, 1, 1).into(), [1; 3]);
    //
    //     assert!(!chunk.is_default());
    // }

    #[test]
    fn is_at_edge() {
        let voxel = (1, 1, 1).into();
        assert!(!super::is_at_edge(voxel));

        let voxel = (1, 0, 1).into();
        assert!(super::is_at_edge(voxel));

        let voxel = (1, Y_END, 1).into();
        assert!(super::is_at_edge(voxel));

        let voxel = (0, 0, 0).into();
        assert!(super::is_at_edge(voxel));

        let voxel = (2, 1, 14).into();
        assert!(!super::is_at_edge(voxel));
    }

    #[test]
    fn path() {
        let chunk = Chunk::new(-1, 9999);
        assert_eq!(chunk.path(), format!("chunk://-1_9999"));
    }
}
