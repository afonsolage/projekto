use crate::{
    coords::{Chunk, ChunkVoxel},
    voxel::{self},
};
use bevy::math::IVec2;
use serde::{Deserialize, Serialize};

mod storage;
pub use storage::*;

mod column;
mod impls;
mod sub_chunk;
mod zip;

// impl Chunk {
//     pub fn new(x: i32, z: i32) -> Self {
//         Self(IVec2::new(x, z))
//     }
//
//     pub fn from_path(path: &std::path::Path) -> Self {
//         let file_name = path
//             .file_name()
//             .expect("To be a valid chunk path")
//             .to_str()
//             .expect("To be UTF-8 string");
//         let (x, z) = file_name
//             .split_once('_')
//             .expect("Chunk path must be composed of X_Z");
//         Self(IVec2::new(
//             x.parse().expect("X on chunk path to be a valid i32"),
//             z.parse().expect("Z on chunk path to be a valid i32"),
//         ))
//     }
//
//     #[inline]
//     pub fn z(&self) -> i32 {
//         self.0.y
//     }
//
//     #[inline]
//     pub fn x(&self) -> i32 {
//         self.0.x
//     }
//
//     #[inline]
//     pub fn xz(&self) -> IVec2 {
//         self.0
//     }
//
//     pub fn neighbor(&self, dir: IVec2) -> Self {
//         Chunk(self.0 + dir)
//     }
//
//     pub fn distance(&self, other: Chunk) -> IVec2 {
//         other.0 - self.0
//     }
//
//     pub fn path(&self) -> String {
//         format!("chunk://{}_{}", self.0.x, self.0.y)
//     }
// }

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

    pub const fn from_dir(dir: IVec2) -> ChunkSide {
        if dir.x == 1 {
            ChunkSide::Right
        } else if dir.x == -1 {
            ChunkSide::Left
        } else if dir.y == 1 {
            ChunkSide::Front
        } else if dir.y == -1 {
            ChunkSide::Back
        } else {
            panic!("Invalid direction received")
        }
    }

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

pub fn voxels() -> impl Iterator<Item = ChunkVoxel> {
    (0..Chunk::BUFFER_SIZE).map(ChunkVoxel::from)
}
pub fn top_voxels() -> impl Iterator<Item = ChunkVoxel> {
    (0..=Chunk::X_END)
        .flat_map(|x| (0..=Chunk::Z_END).map(move |z| ChunkVoxel::new(x, Chunk::Y_END, z)))
}

#[inline]
pub fn is_at_edge(voxel: ChunkVoxel) -> bool {
    voxel.x == 0
        || voxel.y == 0
        || voxel.z == 0
        || voxel.x == (Chunk::X_AXIS_SIZE - 1) as u8
        || voxel.y == (Chunk::Y_AXIS_SIZE - 1) as u8
        || voxel.z == (Chunk::Z_AXIS_SIZE - 1) as u8
}

pub fn overlap_voxel(voxel: ChunkVoxel, dir: IVec2) -> (IVec2, ChunkVoxel) {
    let voxel = IVec2::from(voxel) + dir;

    let overlapping_voxel = ChunkVoxel::new(
        voxel.x.rem_euclid(Chunk::X_AXIS_SIZE as i32) as u8,
        // y never change, since there is no chunk above or bellow
        voxel.y as u8,
        // since IVec2 has only (x, y), we have to use Y.
        voxel.y.rem_euclid(Chunk::Z_AXIS_SIZE as i32) as u8,
    );

    let overlapping_dir = (
        if voxel.x < 0 {
            -1
        } else if voxel.x >= Chunk::X_AXIS_SIZE as i32 {
            1
        } else {
            0
        },
        // since IVec2 has only (x, y), we have to use Y.
        if voxel.y < 0 {
            -1
        } else if voxel.y >= Chunk::Z_AXIS_SIZE as i32 {
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
    use rand::Rng;

    use super::*;

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
        let mut last = ChunkVoxel::default();

        for pos in super::voxels() {
            if first.is_none() {
                first = Some(pos);
            }
            last = pos;
        }

        assert_eq!(first, Some(ChunkVoxel::new(0, 0, 0)));
        assert_eq!(
            last,
            (
                (Chunk::X_AXIS_SIZE as i32 - 1) as u8,
                (Chunk::Y_AXIS_SIZE as i32 - 1) as u8,
                (Chunk::Z_AXIS_SIZE as i32 - 1) as u8
            )
                .into()
        );
    }

    #[test]
    fn top_voxels() {
        let top_voxels = super::top_voxels().collect::<Vec<_>>();

        assert_eq!(top_voxels.len(), Chunk::X_AXIS_SIZE * Chunk::Z_AXIS_SIZE);
        top_voxels
            .into_iter()
            .for_each(|voxel| assert_eq!(voxel.y, Chunk::Y_END));
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

        let voxel = (1, Chunk::Y_END, 1).into();
        assert!(super::is_at_edge(voxel));

        let voxel = (0, 0, 0).into();
        assert!(super::is_at_edge(voxel));

        let voxel = (2, 1, 14).into();
        assert!(!super::is_at_edge(voxel));
    }
}
