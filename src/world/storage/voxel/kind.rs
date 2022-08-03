use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::world::storage::chunk::ChunkStorageType;

use super::{Side, VoxelFace};

#[derive(Debug, Copy, Clone, Deserialize, Default)]
pub struct KindSideTexture {
    pub color: (f32, f32, f32, f32),
    pub offset: IVec2,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub enum KindSidesDesc {
    #[default]
    None,
    All(KindSideTexture),
    Unique {
        right: KindSideTexture,
        left: KindSideTexture,
        up: KindSideTexture,
        down: KindSideTexture,
        front: KindSideTexture,
        back: KindSideTexture,
    },
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct KindDescItem {
    pub name: String,
    pub id: u16,
    pub sides: KindSidesDesc,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct KindsDescs {
    pub atlas_path: String,
    pub atlas_size: u16,
    pub atlas_tile_size: u16,
    pub descriptions: Vec<KindDescItem>,
}

impl KindsDescs {
    pub fn count_tiles(&self) -> u16 {
        self.atlas_size / self.atlas_tile_size
    }
}

impl KindsDescs {
    pub fn get_face_desc(&self, face: &VoxelFace) -> KindSideTexture {
        let kind_desc = self
            .descriptions
            .iter()
            .find(|k| k.id == face.kind.0)
            .map(|desc| desc)
            .expect(format!("Unable to find kind description for face {:?}", face).as_str());

        match kind_desc.sides {
            KindSidesDesc::None => panic!("{} kind should not be rendered.", face.kind.0),
            KindSidesDesc::All(desc) => desc,
            KindSidesDesc::Unique {
                right,
                left,
                up,
                down,
                front,
                back,
            } => match face.side {
                Side::Right => right,
                Side::Left => left,
                Side::Up => up,
                Side::Down => down,
                Side::Front => front,
                Side::Back => back,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default, Deserialize, Serialize)]
pub struct Kind(u16);

impl From<u16> for Kind {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl Into<u16> for Kind {
    fn into(self) -> u16 {
        self.0
    }
}

impl Kind {
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn is_opaque(&self) -> bool {
        // TODO: Implement light emission based on kind descs
        self.0 != 4 && self.0 > 0
    }

    pub fn is_light_emitter(&self) -> bool {
        // TODO: Implement light emission based on kind descs
        self.0 == 4
    }
}

impl ChunkStorageType for Kind {}

#[cfg(test)]
mod tests {
    use ron::de::from_reader;

    use super::*;

    #[test]
    fn load_kind_descriptions() {
        let input_path = format!("{}/assets/voxels/kind.ron", env!("CARGO_MANIFEST_DIR"));
        let f = std::fs::File::open(&input_path).expect("Failed opening kind descriptions file");

        let _: KindsDescs = from_reader(f).unwrap();
    }
}
