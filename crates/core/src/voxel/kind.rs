use std::path::Path;

use bevy_log::trace;
use bevy_math::IVec2;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

use crate::chunk::ChunkStorageType;

use super::{Side, VoxelFace};

static KINDS_DESCS: OnceCell<KindsDescs> = OnceCell::new();

/// Describes what color and offset on texture atlas to be used.
#[derive(Debug, Copy, Clone, Deserialize, Default)]
pub struct KindSideTexture {
    /// RGBA Color in scalar range [0.0 ~ 1.0]
    pub color: (f32, f32, f32, f32),
    /// Texture atlas off set (X, Y)
    pub offset: IVec2,
}

/// Describes how each side of voxel kind should be rendered.
#[derive(Debug, Clone, Deserialize, Default)]
pub enum KindSidesDesc {
    /// Do not render this kind
    #[default]
    None,
    /// All sides are equals and should be rendered the same
    All(KindSideTexture),
    /// Each side has it's own unique behavior
    Unique {
        right: KindSideTexture,
        left: KindSideTexture,
        up: KindSideTexture,
        down: KindSideTexture,
        front: KindSideTexture,
        back: KindSideTexture,
    },
}

/// Describes how this kind should behave when interacting with light.
#[derive(Debug, Clone, Deserialize, Default)]
pub enum KindLightDesc {
    /// No light interaction at all
    #[default]
    None,
    /// Fully blocks light
    Opaque,
    /// Emits given light as artificial light
    Emitter(u8),
}

// TODO: Find a better way to describe this
#[derive(Debug, Clone, Deserialize, Default)]
pub enum KindSourceDesc {
    #[default]
    None,
    Genesis {
        height: i32,
    },
}

/// Describes how this kind should behave on the voxel world.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct KindDescItem {
    pub name: String,
    pub id: u16,
    pub sides: KindSidesDesc,
    pub light: KindLightDesc,
    pub source: KindSourceDesc,
}

/// Holds a list of [`KindDescItem`] and other global data.
/// This struct is create from a ron file.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct KindsDescs {
    pub atlas_path: String,
    pub atlas_size: u16,
    pub atlas_tile_size: u16,
    pub descriptions: Vec<KindDescItem>,
}

impl KindsDescs {
    /// Counts how many tiles there are on a single texture atlas row
    pub fn count_tiles(&self) -> u16 {
        self.atlas_size / self.atlas_tile_size
    }

    /// **Returns** how a given face should be rendered
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

    /// On the first call, this functions reads the ron file and load the [`KindsDescs`] struct from it.
    /// The reading operation is thread-blocking.
    /// This function should be first called on a controlled context to avoid blocking.
    /// Subsequent calls just get a static reference from loaded struct.
    pub fn get() -> &'static Self {
        #[cfg(feature = "auto_load_kinds_descs")]
        if KINDS_DESCS.get().is_none() {
            return Self::init(format!("{}/voxels/kind.ron", env!("ASSETS_PATH")));
        }
        
        KINDS_DESCS
            .get()
            .expect("KindsDescs should be initialized before used")
    }

    pub fn init(path: impl AsRef<Path>) -> &'static Self {
        trace!(
            "Loading kinds descriptions on path {:?}",
            path.as_ref().as_os_str()
        );
        match std::fs::File::open(&path) {
            Ok(file) => {
                let kinds_descs: KindsDescs = ron::de::from_reader(file).unwrap();
                KINDS_DESCS.set(kinds_descs).ok();
                Self::get()
            }
            Err(e) => {
                let path = path.as_ref().to_str().unwrap();
                panic!("Failed to init kinds descriptions on path {path}. Error: {e}");
            }
        }
    }
}

/// Kind id reference.
/// This function uses [`KindsDescs`] to determine how this kind should behave.
/// May panic if current kind id doesn't exists on [`KindsDescs`].
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
    /// Creates a new [`Kind`] with the given id
    pub fn id(id: u16) -> Self {
        Kind(id)
    }

    /// Creates a new [`Kind`] with id 0.
    pub fn none() -> Self {
        Kind(0)
    }

    /// Checks if current kind is the None [`Kind`], which has id 0.
    pub fn is_none(&self) -> bool {
        self.0 == 0
    }

    /// Checks if current kind is [`KindLightDesc::Opaque`].
    pub fn is_opaque(&self) -> bool {
        match self.desc().light {
            KindLightDesc::Opaque => true,
            _ => false,
        }
    }

    /// Checks if current kind is [`KindLightDesc::Emitter`].
    pub fn is_light_emitter(&self) -> bool {
        match self.desc().light {
            KindLightDesc::Emitter(_) => true,
            _ => false,
        }
    }

    /// **Returns** the light intensity emitted by this kind or zero if it isn't a [`KindLightDesc::Emitter`]
    pub fn light_emission(&self) -> u8 {
        match self.desc().light {
            KindLightDesc::Emitter(intensity) => intensity,
            _ => 0,
        }
    }

    // TODO: rework this, to use noise layers and get generated kind for each layer
    pub fn get_kind_with_height_source(surface: usize, height: usize) -> Self {
        let depth = height as i32 - surface as i32;

        match depth {
            depth if depth == 0 => Kind(2),
            depth if depth >= -3 && depth <= -1 => Kind(1),
            _ => Kind(3),
        }
    }

    /// Get the [`KindDescItem`] corresponding to this kind id.
    /// Panics with there is no kind id on [`KindsDescs`] global reference.
    fn desc(&self) -> &KindDescItem {
        for desc in KindsDescs::get().descriptions.iter() {
            if desc.id == self.0 {
                return desc;
            }
        }

        panic!("Failed to find kind description {}", self.0);
    }
}

impl ChunkStorageType for Kind {}

#[cfg(test)]
mod tests {
    use ron::de::from_reader;

    use super::*;

    #[test]
    fn load_kind_descriptions() {
        let input_path = format!("{}/voxels/kind.ron", env!("ASSETS_PATH"));
        let f = std::fs::File::open(&input_path).expect("Failed opening kind descriptions file");

        let _: KindsDescs = from_reader(f).unwrap();
    }
}
