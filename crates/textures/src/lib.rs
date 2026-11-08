//! Procedural terrain-atlas generation prototype.
//!
//! The client renders terrain from a 320x320 atlas and the mesher bakes tile coordinates into
//! chunk vertices. This crate generates that atlas from a world seed and code alone: there is no
//! spritesheet, bitmap or other image input. The atlas layout, UVs and mesh format stay unchanged,
//! and the protocol only gains an append-only `ServerInfo` message, while every seed produces a
//! slightly different tile set.
//!
//! Two synthesis styles share one pipeline:
//!
//! - [`TextureStyle::Noise`]: periodic fractal noise mapped through an authored palette.
//! - [`TextureStyle::Structural`]: layered structural synthesis (tufts, cracks, strata, ripples,
//!   lichen, sparkles) and the style the project should ship.
//!
//! An [`ArtPreset`] then decides how the result looks (shade count, palette contrast, chroma,
//! temperature, relief strength). Run the `gen_atlas` binary to write an atlas and dumped tiles.

pub mod art;
pub mod color;
pub mod hash;
pub mod index_tile;
pub mod layers;
pub mod materials;
pub mod noise;
pub mod styles;

use art::ArtParameters;
pub use art::ArtPreset;
use index_tile::TILE;
use materials::Material;

/// Number of tiles along one atlas edge, matching `kind.ron`'s `atlas_size / atlas_tile_size`.
pub const ATLAS_TILES: usize = 10;
/// Atlas edge in pixels.
pub const ATLAS_SIZE: usize = TILE * ATLAS_TILES;

/// A generated atlas: full RGBA pixels plus the individual tiles for inspection and tests.
pub struct GeneratedAtlas {
    pub style: TextureStyle,
    pub art: ArtPreset,
    pub seed: u64,
    pub rgba: Vec<u8>,
    pub tiles: Vec<GeneratedTile>,
}

/// One generated tile inside an atlas.
pub struct GeneratedTile {
    pub name: &'static str,
    pub offset: (u32, u32),
    pub rgba: Vec<u8>,
}

/// The synthesis style: how much structure the generators add.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextureStyle {
    /// Periodic noise plus palette, with only the structure each material needs to read.
    Noise,
    /// Layered structural synthesis with material motifs. Expected to ship.
    #[default]
    Structural,
}

impl TextureStyle {
    pub const ALL: [Self; 2] = [Self::Noise, Self::Structural];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "noise" => Some(Self::Noise),
            "structural" => Some(Self::Structural),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Noise => "noise",
            Self::Structural => "structural",
        }
    }
}

/// Generates the full atlas for `style`, `art` direction and world `seed`.
///
/// Generation is pure: no bitmap input, no I/O and no randomness outside the seed.
pub fn generate_atlas(style: TextureStyle, art: ArtPreset, seed: u64) -> GeneratedAtlas {
    let params = art.parameters();
    let mut rgba = vec![0u8; ATLAS_SIZE * ATLAS_SIZE * 4];
    let mut tiles = Vec::with_capacity(materials::MATERIALS.len());

    for material in &materials::MATERIALS {
        let ramps = material_ramps(material, seed, &params);
        let detail = match style {
            TextureStyle::Noise => styles::structural::Detail::Noise,
            TextureStyle::Structural => styles::structural::Detail::Full,
        };
        let content = styles::structural::tile(material, seed, detail, &params);
        let tile_rgba = content.to_rgba(&ramps);
        debug_assert_eq!(tile_rgba.len(), TILE * TILE * 4);
        blit(&mut rgba, material.offset, &tile_rgba);
        tiles.push(GeneratedTile {
            name: material.name,
            offset: material.offset,
            rgba: tile_rgba,
        });
    }

    GeneratedAtlas {
        style,
        art,
        seed,
        rgba,
        tiles,
    }
}

/// Palettes used by a material, one per layer; simple materials have exactly one.
fn material_ramps(material: &Material, seed: u64, art: &ArtParameters) -> Vec<color::Ramp> {
    if material.is_composite() {
        material
            .layers
            .iter()
            .map(|layer| materials::by_kind(layer.source).ramp(seed, art))
            .collect()
    } else {
        vec![material.ramp(seed, art)]
    }
}

fn blit(atlas: &mut [u8], offset: (u32, u32), tile: &[u8]) {
    let origin_x = offset.0 as usize * TILE;
    let origin_y = offset.1 as usize * TILE;
    for row in 0..TILE {
        let destination = ((origin_y + row) * ATLAS_SIZE + origin_x) * 4;
        let source = row * TILE * 4;
        atlas[destination..destination + TILE * 4]
            .copy_from_slice(&tile[source..source + TILE * 4]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_has_the_layout_the_mesher_expects() {
        let atlas = generate_atlas(TextureStyle::Structural, ArtPreset::default(), 42);
        assert_eq!(atlas.rgba.len(), 320 * 320 * 4);
        assert_eq!(atlas.rgba.len(), ATLAS_SIZE * ATLAS_SIZE * 4);

        for tile in &atlas.tiles {
            let origin =
                ((tile.offset.1 as usize * TILE) * ATLAS_SIZE + tile.offset.0 as usize * TILE) * 4;
            assert_eq!(&atlas.rgba[origin..origin + 4], &tile.rgba[0..4]);
            assert_eq!(tile.rgba[3], 255, "{} must be opaque", tile.name);
        }
    }

    #[test]
    fn generation_is_deterministic() {
        for style in TextureStyle::ALL {
            let first = generate_atlas(style, ArtPreset::Soft, 42);
            let second = generate_atlas(style, ArtPreset::Soft, 42);
            assert_eq!(
                first.rgba,
                second.rgba,
                "{} must be deterministic",
                style.name()
            );
        }
    }

    #[test]
    fn different_seeds_give_different_atlases() {
        let first = generate_atlas(TextureStyle::Structural, ArtPreset::Soft, 42);
        let second = generate_atlas(TextureStyle::Structural, ArtPreset::Soft, 43);
        assert_ne!(first.rgba, second.rgba);
    }

    #[test]
    fn art_presets_change_the_look() {
        let soft = generate_atlas(TextureStyle::Structural, ArtPreset::Soft, 42);
        let pixel = generate_atlas(TextureStyle::Structural, ArtPreset::Pixel, 42);
        assert_eq!(soft.rgba.len(), pixel.rgba.len());
        assert_ne!(soft.rgba, pixel.rgba);
    }

    fn tile<'a>(atlas: &'a GeneratedAtlas, name: &str) -> &'a GeneratedTile {
        atlas
            .tiles
            .iter()
            .find(|tile| tile.name == name)
            .expect("tile exists")
    }

    fn pixel(tile: &GeneratedTile, x: usize, y: usize) -> [u8; 3] {
        let index = (y * TILE + x) * 4;
        [tile.rgba[index], tile.rgba[index + 1], tile.rgba[index + 2]]
    }

    fn colors(tile: &GeneratedTile) -> std::collections::HashSet<[u8; 3]> {
        (0..TILE * TILE)
            .map(|index| pixel(tile, index % TILE, index / TILE))
            .collect()
    }

    #[test]
    fn grass_side_composites_grass_over_dirt() {
        let atlas = generate_atlas(TextureStyle::Structural, ArtPreset::Soft, 42);
        let grass = tile(&atlas, "grass_top");
        let dirt = tile(&atlas, "dirt");
        let side = tile(&atlas, "grass_side");

        let grass_colors = colors(grass);
        let dirt_colors = colors(dirt);

        // The top row belongs to the grass band and the bottom row to the dirt base, which proves
        // the composite resolves each region through the matching source palette.
        assert!(
            grass_colors.contains(&pixel(side, 16, 0)),
            "top row is not grass"
        );
        assert!(
            dirt_colors.contains(&pixel(side, 16, TILE - 1)),
            "bottom row is not dirt"
        );
    }

    #[test]
    fn three_layer_material_uses_three_palettes() {
        let atlas = generate_atlas(TextureStyle::Structural, ArtPreset::Soft, 42);
        let snowy = tile(&atlas, "snowy_grass_side");
        let color_count = colors(snowy).len();
        assert!(
            color_count >= 3,
            "expected dirt, grass and snow shades, got {color_count}"
        );
    }

    #[test]
    fn styles_differ() {
        let noise = generate_atlas(TextureStyle::Noise, ArtPreset::Soft, 42);
        let structural = generate_atlas(TextureStyle::Structural, ArtPreset::Soft, 42);
        assert_ne!(noise.rgba, structural.rgba);
    }
}
