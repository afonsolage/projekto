//! Palette-indexed tile buffer.
//!
//! Tile content is generated as small integers (`0..GEN_LEVELS`), composed, shaded and finally
//! mapped through a [`Ramp`] to sRGB bytes. Keeping the content indexed means every feature
//! stays inside the authored palette, which is what gives the result a pixel-art look and stops
//! structure layers from introducing new colors.

use crate::color::{GEN_LEVELS, Ramp};

pub const TILE: usize = 32;
pub const TILE_PIXELS: usize = TILE * TILE;
/// Pixels closer than this to the border stay free of one-off stamps so the atlas tile repeats.
pub const MARGIN: usize = 2;

/// A 32x32 grid of palette indices.
#[derive(Clone, Debug)]
pub struct IndexTile {
    data: [u8; TILE_PIXELS],
}

impl IndexTile {
    /// Creates a tile filled with `level`.
    pub fn filled(level: u8) -> Self {
        Self {
            data: [level.min((GEN_LEVELS - 1) as u8); TILE_PIXELS],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.data[y * TILE + x]
    }

    pub fn set(&mut self, x: usize, y: usize, level: i32) {
        self.data[y * TILE + x] = level.clamp(0, (GEN_LEVELS - 1) as i32) as u8;
    }

    pub fn add(&mut self, x: usize, y: usize, delta: i32) {
        let level = self.data[y * TILE + x] as i32 + delta;
        self.data[y * TILE + x] = level.clamp(0, (GEN_LEVELS - 1) as i32) as u8;
    }

    /// Reads a pixel with wrapped coordinates, matching how the shader repeats the tile.
    pub fn wrap_get(&self, x: i32, y: i32) -> u8 {
        let x = x.rem_euclid(TILE as i32) as usize;
        let y = y.rem_euclid(TILE as i32) as usize;
        self.data[y * TILE + x]
    }

    /// Shades the tile from its own height field, with light coming from the top-left.
    ///
    /// `strength` around `0.25` adds a subtle bevel; higher values read as stronger relief.
    pub fn bevel(&mut self, strength: f32) {
        let source = self.clone();
        let height = |x: i32, y: i32| source.wrap_get(x, y) as f32;

        for y in 0..TILE {
            for x in 0..TILE {
                let x = x as i32;
                let y = y as i32;

                let gx = height(x - 1, y - 1) + 2.0 * height(x - 1, y) + height(x - 1, y + 1)
                    - height(x + 1, y - 1)
                    - 2.0 * height(x + 1, y)
                    - height(x + 1, y + 1);
                let gy = height(x - 1, y - 1) + 2.0 * height(x, y - 1) + height(x + 1, y - 1)
                    - height(x - 1, y + 1)
                    - 2.0 * height(x, y + 1)
                    - height(x + 1, y + 1);
                let delta = ((gx + gy) * 0.05 * strength).round() as i32;
                self.add(x as usize, y as usize, delta);
            }
        }
    }

    /// Maps the tile through `ramp` into RGBA bytes.
    pub fn to_rgba(&self, ramp: &Ramp) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(TILE_PIXELS * 4);
        for &level in &self.data {
            let [r, g, b] = ramp.srgb_for_index(level);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
        rgba
    }

    /// Histogram of palette levels, used by tests to check tiles are not flat.
    pub fn levels(&self) -> [u32; GEN_LEVELS] {
        let mut counts = [0; GEN_LEVELS];
        for &level in &self.data {
            counts[level as usize] += 1;
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{GEN_LEVELS, Oklch, PaletteJitter, PaletteTransform};

    #[test]
    fn set_clamps_to_palette() {
        let mut tile = IndexTile::filled(0);
        tile.set(0, 0, 99);
        tile.add(1, 0, -50);
        assert_eq!(tile.get(0, 0) as usize, GEN_LEVELS - 1);
        assert_eq!(tile.get(1, 0), 0);
    }

    #[test]
    fn wrap_get_wraps_both_axes() {
        let mut tile = IndexTile::filled(0);
        tile.set(0, 0, 3);
        assert_eq!(tile.wrap_get(-32, -32), 3);
        assert_eq!(tile.wrap_get(32, 0), 3);
    }

    #[test]
    fn rgba_uses_ramp() {
        let ramp = Ramp::new(
            Oklch::new(0.5, 0.1, 90.0),
            0.1,
            GEN_LEVELS,
            PaletteTransform::default(),
            PaletteJitter::default(),
        );
        let tile = IndexTile::filled(3);
        let rgba = tile.to_rgba(&ramp);
        assert_eq!(rgba.len(), TILE_PIXELS * 4);
        assert_eq!(&rgba[0..3], &ramp.srgb_for_index(3));
        assert_eq!(rgba[3], 255);
    }

    #[test]
    fn layered_content_uses_each_palette() {
        let first = Ramp::new(
            Oklch::new(0.5, 0.1, 90.0),
            0.1,
            GEN_LEVELS,
            PaletteTransform::default(),
            PaletteJitter::default(),
        );
        let second = Ramp::new(
            Oklch::new(0.4, 0.15, 30.0),
            0.1,
            GEN_LEVELS,
            PaletteTransform::default(),
            PaletteJitter::default(),
        );
        let mut levels = IndexTile::filled(2);
        levels.set(1, 1, 4);
        let mut palette = [0u8; TILE_PIXELS];
        palette[TILE + 1] = 1;
        let rgba = TileContent::layered(levels, palette).to_rgba(&[first.clone(), second.clone()]);
        assert_eq!(
            &rgba[(TILE + 1) * 4..(TILE + 1) * 4 + 3],
            &second.srgb_for_index(4)
        );
        assert_eq!(&rgba[0..3], &first.srgb_for_index(2));
    }
}

/// A generated tile: palette-indexed content with a per-pixel palette selection.
#[derive(Clone, Debug)]
pub enum TileContent {
    /// One palette ramp for the whole tile.
    Indexed(IndexTile),
    /// Each pixel resolves through the ramp selected by `palette` (one entry per layer).
    Layered {
        levels: IndexTile,
        palette: Box<[u8; TILE_PIXELS]>,
    },
}

impl TileContent {
    /// Builds a layered tile from levels and a per-pixel palette index.
    pub fn layered(levels: IndexTile, palette: [u8; TILE_PIXELS]) -> Self {
        Self::Layered {
            levels,
            palette: Box::new(palette),
        }
    }

    /// Maps the content to RGBA bytes. `ramps` are the palettes of the material's layers, in the
    /// same order as the layer stack; a single-layer material passes one ramp.
    pub fn to_rgba(&self, ramps: &[Ramp]) -> Vec<u8> {
        match self {
            Self::Indexed(tile) => tile.to_rgba(&ramps[0]),
            Self::Layered { levels, palette } => {
                let mut rgba = Vec::with_capacity(TILE_PIXELS * 4);
                for (index, &level) in levels.data.iter().enumerate() {
                    let ramp = &ramps[palette[index] as usize];
                    let [r, g, b] = ramp.srgb_for_index(level);
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
                rgba
            }
        }
    }
}
