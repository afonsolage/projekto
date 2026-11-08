//! Authored material descriptions: atlas slot, palette anchors and layer stacks.
//!
//! A material is either simple (one procedural pattern, one palette) or a composite of several
//! materials drawn through procedural masks. The offsets mirror `assets/voxels/kind.ron`, so a
//! generated atlas can replace the shipped PNG without touching the mesher. Complex blocks are
//! handled at two levels:
//!
//! - `kind.ron` binds a different material to each face (`KindSidesDesc::Unique`), which is how a
//!   grass block gets grass on top, dirt on the bottom and a mixed material on the sides.
//! - A composite material stacks patterns from simple materials with a [`LayerMask`], so a side
//!   tile can be "dirt with a grass band on top" without a separate authored alpha mask.

use crate::art::ArtParameters;
use crate::color::{Oklch, PaletteJitter, PaletteTransform, Ramp};
use crate::layers::{LayerMask, Patches, TopBand};

/// What a tile depicts. Drives which generator runs for simple materials and which pattern a
/// composite layer reuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialKind {
    Dirt,
    GrassTop,
    GrassSide,
    Rock,
    Sand,
    Snow,
    Moss,
    Lamp,
    SnowSide,
    MossSide,
    MossyRock,
    SnowyGrassSide,
}

/// Climate coordinates of the biome that owns the material, copied from `assets/worldgen.ron`.
///
/// They are carried so per-biome palettes can later be derived from the same fields the world
/// generator samples.
#[derive(Clone, Copy, Debug)]
pub struct Climate {
    pub temperature: f32,
    pub humidity: f32,
    pub erosion: f32,
}

/// One pattern drawn over the layers below it.
#[derive(Clone, Copy, Debug)]
pub struct Layer {
    /// Simple material whose pattern and palette are reused.
    pub source: MaterialKind,
    /// Where the layer may draw.
    pub mask: LayerMask,
}

impl Layer {
    pub const fn new(source: MaterialKind, mask: LayerMask) -> Self {
        Self { source, mask }
    }
}

/// One generated tile description.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub kind: MaterialKind,
    pub name: &'static str,
    /// Atlas cell `(column, row)`, matching the mesher's tile coordinates.
    pub offset: (u32, u32),
    /// Base color of the palette ramp, in Oklch. Only used by simple materials.
    pub base: Oklch,
    /// Palette spread: maximum lightness offset from `base`. Only used by simple materials.
    pub spread: f32,
    /// Seed derivation tag, so each material jitters independently. Only used by simple
    /// materials.
    pub tag: u64,
    /// Biome climate that owns this material.
    pub climate: Climate,
    /// Layer stack. Empty for simple materials; the first layer is the base and must use
    /// [`LayerMask::Full`].
    pub layers: &'static [Layer],
}

impl Material {
    /// Builds the seeded palette ramp for this simple material under the given art direction.
    pub fn ramp(&self, seed: u64, art: &ArtParameters) -> Ramp {
        let jitter = PaletteJitter::from_seed(seed, self.tag);
        let transform = PaletteTransform {
            contrast: art.contrast,
            chroma: art.chroma,
            temperature: art.temperature,
            highlight_boost: art.highlight_boost,
        };
        Ramp::new(self.base, self.spread, art.levels, transform, jitter)
    }

    pub fn is_composite(&self) -> bool {
        !self.layers.is_empty()
    }
}

const FULL: LayerMask = LayerMask::Full;

/// Dirt block: one flat pattern on every face.
const LAYERS_DIRT: &[Layer] = &[];

/// Grass top: a single grass pattern.
const LAYERS_GRASS_TOP: &[Layer] = &[];

/// Grass side: dirt with a grass band on top, matching the grass top pattern and palette.
const LAYERS_GRASS_SIDE: &[Layer] = &[
    Layer::new(MaterialKind::Dirt, FULL),
    Layer::new(
        MaterialKind::GrassTop,
        LayerMask::TopBand(TopBand {
            height: 8.0,
            variation: 4.5,
            drips: 20,
            lip: true,
            highlight_top: true,
            seed_tag: 0x24,
        }),
    ),
];

/// Tundra surface side: dirt with a snow cap on top.
const LAYERS_SNOW_SIDE: &[Layer] = &[
    Layer::new(MaterialKind::Dirt, FULL),
    Layer::new(
        MaterialKind::Snow,
        LayerMask::TopBand(TopBand {
            height: 6.0,
            variation: 3.0,
            drips: 25,
            lip: true,
            highlight_top: true,
            seed_tag: 0x52,
        }),
    ),
];

/// Forest surface side: dirt with a moss band on top.
const LAYERS_MOSS_SIDE: &[Layer] = &[
    Layer::new(MaterialKind::Dirt, FULL),
    Layer::new(
        MaterialKind::Moss,
        LayerMask::TopBand(TopBand {
            height: 7.0,
            variation: 3.5,
            drips: 0,
            lip: false,
            highlight_top: true,
            seed_tag: 0x53,
        }),
    ),
];

/// Overgrown rock: rock with organic moss patches.
const LAYERS_MOSSY_ROCK: &[Layer] = &[
    Layer::new(MaterialKind::Rock, FULL),
    Layer::new(
        MaterialKind::Moss,
        LayerMask::Patches(Patches {
            threshold: 0.12,
            cells: 3,
            octaves: 3,
            seed_tag: 0x62,
        }),
    ),
];

/// Snow-covered grass side: three layers, snow over grass over dirt.
///
/// This is the case that shows why the layer stack is the right abstraction: adding a snow cap to
/// a grassy slope is a data change, not a new generator.
const LAYERS_SNOWY_GRASS_SIDE: &[Layer] = &[
    Layer::new(MaterialKind::Dirt, FULL),
    Layer::new(
        MaterialKind::GrassTop,
        LayerMask::TopBand(TopBand {
            height: 9.0,
            variation: 4.0,
            drips: 16,
            lip: true,
            highlight_top: false,
            seed_tag: 0x24,
        }),
    ),
    Layer::new(
        MaterialKind::Snow,
        LayerMask::TopBand(TopBand {
            height: 3.5,
            variation: 2.0,
            drips: 30,
            lip: true,
            highlight_top: true,
            seed_tag: 0x54,
        }),
    ),
];

/// The generated tile set, in generation order.
pub const MATERIALS: [Material; 12] = [
    Material {
        kind: MaterialKind::Dirt,
        name: "dirt",
        offset: (2, 1),
        base: Oklch::new(0.440, 0.115, 52.0),
        spread: 0.095,
        tag: 2,
        climate: Climate {
            temperature: 0.0,
            humidity: 0.0,
            erosion: 0.2,
        },
        layers: LAYERS_DIRT,
    },
    Material {
        kind: MaterialKind::GrassTop,
        name: "grass_top",
        offset: (1, 0),
        base: Oklch::new(0.355, 0.125, 143.0),
        spread: 0.100,
        tag: 3,
        climate: Climate {
            temperature: 0.0,
            humidity: 0.0,
            erosion: 0.2,
        },
        layers: LAYERS_GRASS_TOP,
    },
    Material {
        kind: MaterialKind::GrassSide,
        name: "grass_side",
        offset: (0, 1),
        base: Oklch::new(0.440, 0.115, 52.0),
        spread: 0.095,
        tag: 2,
        climate: Climate {
            temperature: 0.0,
            humidity: 0.0,
            erosion: 0.2,
        },
        layers: LAYERS_GRASS_SIDE,
    },
    Material {
        kind: MaterialKind::Rock,
        name: "rock",
        offset: (2, 0),
        base: Oklch::new(0.495, 0.006, 250.0),
        spread: 0.095,
        tag: 5,
        climate: Climate {
            temperature: 0.0,
            humidity: 0.0,
            erosion: -0.4,
        },
        layers: &[],
    },
    Material {
        kind: MaterialKind::Sand,
        name: "sand",
        offset: (3, 0),
        base: Oklch::new(0.800, 0.090, 88.0),
        spread: 0.090,
        tag: 6,
        climate: Climate {
            temperature: 0.4,
            humidity: -0.4,
            erosion: 0.2,
        },
        layers: &[],
    },
    Material {
        kind: MaterialKind::Snow,
        name: "snow",
        offset: (4, 0),
        base: Oklch::new(0.900, 0.020, 245.0),
        spread: 0.100,
        tag: 7,
        climate: Climate {
            temperature: -0.4,
            humidity: 0.0,
            erosion: 0.2,
        },
        layers: &[],
    },
    Material {
        kind: MaterialKind::Moss,
        name: "moss",
        offset: (5, 0),
        base: Oklch::new(0.380, 0.090, 142.0),
        spread: 0.090,
        tag: 8,
        climate: Climate {
            temperature: 0.0,
            humidity: 0.4,
            erosion: 0.2,
        },
        layers: &[],
    },
    Material {
        kind: MaterialKind::Lamp,
        name: "lamp",
        offset: (1, 1),
        base: Oklch::new(0.690, 0.150, 88.0),
        spread: 0.300,
        tag: 9,
        climate: Climate {
            temperature: 0.0,
            humidity: 0.0,
            erosion: 0.2,
        },
        layers: &[],
    },
    Material {
        kind: MaterialKind::SnowSide,
        name: "snow_side",
        offset: (6, 0),
        base: Oklch::new(0.440, 0.115, 52.0),
        spread: 0.095,
        tag: 2,
        climate: Climate {
            temperature: -0.4,
            humidity: 0.0,
            erosion: 0.2,
        },
        layers: LAYERS_SNOW_SIDE,
    },
    Material {
        kind: MaterialKind::MossSide,
        name: "moss_side",
        offset: (7, 0),
        base: Oklch::new(0.440, 0.115, 52.0),
        spread: 0.095,
        tag: 2,
        climate: Climate {
            temperature: 0.0,
            humidity: 0.4,
            erosion: 0.2,
        },
        layers: LAYERS_MOSS_SIDE,
    },
    Material {
        kind: MaterialKind::MossyRock,
        name: "mossy_rock",
        offset: (8, 0),
        base: Oklch::new(0.495, 0.006, 250.0),
        spread: 0.095,
        tag: 5,
        climate: Climate {
            temperature: 0.0,
            humidity: 0.4,
            erosion: -0.4,
        },
        layers: LAYERS_MOSSY_ROCK,
    },
    Material {
        kind: MaterialKind::SnowyGrassSide,
        name: "snowy_grass_side",
        offset: (9, 0),
        base: Oklch::new(0.440, 0.115, 52.0),
        spread: 0.095,
        tag: 2,
        climate: Climate {
            temperature: -0.4,
            humidity: 0.0,
            erosion: 0.2,
        },
        layers: LAYERS_SNOWY_GRASS_SIDE,
    },
];

/// Looks up a material by kind.
pub fn by_kind(kind: MaterialKind) -> &'static Material {
    MATERIALS
        .iter()
        .find(|material| material.kind == kind)
        .expect("all kinds are present in MATERIALS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_sources_are_simple() {
        for material in &MATERIALS {
            for layer in material.layers {
                let source = by_kind(layer.source);
                assert!(
                    !source.is_composite(),
                    "{} references composite material {} as a layer source",
                    material.name,
                    source.name
                );
            }
        }
    }

    #[test]
    fn composites_start_with_a_full_base() {
        for material in &MATERIALS {
            if let Some(base) = material.layers.first() {
                assert!(
                    matches!(base.mask, LayerMask::Full),
                    "{} must start with a full base layer",
                    material.name
                );
                assert!(!material.layers.is_empty());
            }
        }
    }
}
