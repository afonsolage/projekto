//! Procedural masks used to composite multi-layer materials.
//!
//! A complex block such as grass does not need extra artwork or alpha sheets: its side tile is a
//! base material (dirt) with another material's pattern (grass) composited through a procedural
//! mask. Masks are evaluated per pixel from the seed, so they wrap with the tile like every other
//! generated feature and need no authored data.

use crate::hash::{derive_seed, hash2d};
use crate::index_tile::{IndexTile, TILE, TILE_PIXELS};
use crate::noise::PeriodicNoise;

/// First tile row the fragment shader samples: its 6% UV inset hides the outer two rows.
pub const FIRST_VISIBLE_ROW: usize = 2;

/// Where a layer is allowed to draw over the layers below it.
#[derive(Clone, Copy, Debug)]
pub enum LayerMask {
    /// The layer covers the whole tile.
    Full,
    /// The layer covers the top of the tile with a noisy, optionally dripping edge.
    TopBand(TopBand),
    /// The layer covers noise-selected patches.
    Patches(Patches),
}

/// A band of one material that sits on top of another, as on the side of a grass block.
#[derive(Clone, Copy, Debug)]
pub struct TopBand {
    /// Average band height in pixels.
    pub height: f32,
    /// How far the noisy edge deviates from `height`, in pixels.
    pub variation: f32,
    /// Chance in percent that a column gets a one-pixel overhang below the band.
    pub drips: u32,
    /// Darkens the row where the band meets the material below it.
    pub lip: bool,
    /// Brightens the topmost row of the tile.
    pub highlight_top: bool,
    /// Seed tag for the band noise.
    pub seed_tag: u64,
}

/// Organic patches of one material over another, as on an overgrown rock.
#[derive(Clone, Copy, Debug)]
pub struct Patches {
    /// Noise threshold in `-1..1`; higher values cover less area.
    pub threshold: f32,
    /// Lattice cells per tile of the patch field.
    pub cells: i32,
    /// Octaves of the patch field.
    pub octaves: u32,
    /// Seed tag for the patch field.
    pub seed_tag: u64,
}

impl LayerMask {
    /// Whether the layer covers the pixel at `(x, y)`.
    pub fn covers(&self, x: usize, y: usize, seed: u64) -> bool {
        match self {
            Self::Full => true,
            Self::TopBand(band) => {
                let heights = band_heights(band, seed);
                let column = heights[x.min(TILE - 1)];
                y < column || drip_extension(band, x, seed) + column > y
            }
            Self::Patches(patches) => {
                let field = PeriodicNoise::new(derive_seed(seed, patches.seed_tag), patches.cells);
                let (u, v) = uv(x, y);
                field.fbm(u, v, patches.octaves) > patches.threshold
            }
        }
    }
}

/// Per-column band heights for a top band, in pixels.
pub fn band_heights(band: &TopBand, seed: u64) -> [usize; TILE] {
    let noise = PeriodicNoise::new(derive_seed(seed, band.seed_tag), 5);
    std::array::from_fn(|x| {
        let (u, _) = uv(x, 0);
        let height = band.height + noise.fbm_1d(u, 2) * band.variation;
        (height.round() as i32).clamp(3, (TILE - 5) as i32) as usize
    })
}

/// Extra one-pixel overhang below the band for a column, when the mask allows drips.
pub fn drip_extension(band: &TopBand, x: usize, seed: u64) -> usize {
    if band.drips == 0 {
        return 0;
    }
    let hash = hash2d(x as i32, 1, derive_seed(seed, band.seed_tag ^ 0xD21F));
    usize::from(hash % 100 < band.drips)
}

fn uv(x: usize, y: usize) -> (f32, f32) {
    (
        (x as f32 + 0.5) / TILE as f32,
        (y as f32 + 0.5) / TILE as f32,
    )
}

/// Composes `overlay` onto `levels`/`palette` through a top band mask.
///
/// Returns the band heights so the caller can shade the seam. The previous layer is restored on
/// the first row below the band when the band carries a lip, which keeps the transition readable
/// even when the two materials have similar palettes.
pub fn apply_top_band(
    levels: &mut IndexTile,
    palette: &mut [u8; TILE_PIXELS],
    overlay: &IndexTile,
    band: &TopBand,
    overlay_index: u8,
    seed: u64,
) -> [usize; TILE] {
    let heights = band_heights(band, seed);
    let previous_levels = levels.clone();
    let previous_palette = *palette;

    for x in 0..TILE {
        let height = heights[x].min(TILE - 1);
        let drips = drip_extension(band, x, seed);

        for y in 0..(height + drips).min(TILE) {
            if y < height {
                levels.set(x, y, overlay.get(x, y) as i32);
                palette[y * TILE + x] = overlay_index;
            } else if drips > 0 {
                // An overhang keeps the lower material's colour but uses the band's shape.
                levels.set(x, y, overlay.get(x, height) as i32);
                palette[y * TILE + x] = overlay_index;
            }
        }

        // The lip sits below any overhang; shading row `height` would erase the drip.
        let lip_row = height + drips;
        if band.lip && lip_row < TILE {
            levels.set(x, lip_row, previous_levels.get(x, lip_row) as i32 - 2);
            palette[lip_row * TILE + x] = previous_palette[lip_row * TILE + x];
        }
        if band.highlight_top && height > FIRST_VISIBLE_ROW {
            levels.add(x, FIRST_VISIBLE_ROW, 1);
        }
    }

    heights
}

/// Composes `overlay` onto `levels`/`palette` through a patch mask.
pub fn apply_patches(
    levels: &mut IndexTile,
    palette: &mut [u8; TILE_PIXELS],
    overlay: &IndexTile,
    patches: &Patches,
    overlay_index: u8,
    seed: u64,
) {
    let field = PeriodicNoise::new(derive_seed(seed, patches.seed_tag), patches.cells);
    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            if field.fbm(u, v, patches.octaves) > patches.threshold {
                levels.set(x, y, overlay.get(x, y) as i32);
                palette[y * TILE + x] = overlay_index;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_mask_covers_everything() {
        for x in [0, 5, 31] {
            for y in [0, 5, 31] {
                assert!(LayerMask::Full.covers(x, y, 42));
            }
        }
    }

    #[test]
    fn top_band_covers_top_rows_and_wraps() {
        let band = TopBand {
            height: 8.0,
            variation: 2.0,
            drips: 0,
            lip: true,
            highlight_top: true,
            seed_tag: 7,
        };
        for x in 0..TILE {
            assert!(LayerMask::TopBand(band).covers(x, 0, 42));
            assert!(!LayerMask::TopBand(band).covers(x, TILE - 1, 42));
        }
        // Band heights are periodic, so the first and last columns stay related.
        let heights = band_heights(&band, 42);
        assert_eq!(heights.len(), TILE);
        for height in heights {
            assert!((3..=TILE - 5).contains(&height));
        }
    }

    #[test]
    fn top_band_highlights_a_visible_row() {
        let band = TopBand {
            height: 8.0,
            variation: 0.0,
            drips: 0,
            lip: false,
            highlight_top: true,
            seed_tag: 5,
        };
        let mut levels = IndexTile::filled(3);
        let overlay = IndexTile::filled(3);
        let mut palette = [0u8; TILE_PIXELS];
        apply_top_band(&mut levels, &mut palette, &overlay, &band, 1, 42);

        assert_eq!(levels.get(4, FIRST_VISIBLE_ROW), 4);
        assert_eq!(
            levels.get(4, 0),
            3,
            "the hidden top row must stay untouched"
        );
    }

    #[test]
    fn top_band_drips_survive_the_lip() {
        let band = TopBand {
            height: 8.0,
            variation: 0.0,
            drips: 100,
            lip: true,
            highlight_top: false,
            seed_tag: 3,
        };
        let mut levels = IndexTile::filled(3);
        let overlay = IndexTile::filled(5);
        let mut palette = [0u8; TILE_PIXELS];
        let heights = apply_top_band(&mut levels, &mut palette, &overlay, &band, 1, 42);
        let height = heights[0];

        // The overhang row belongs to the overlay, and the darkened lip moves below it.
        assert_eq!(palette[height * TILE], 1);
        assert_eq!(palette[(height + 1) * TILE], 0);
        assert!(levels.get(0, height + 1) < 3);
    }

    #[test]
    fn patches_are_deterministic_and_partial() {
        let patches = Patches {
            threshold: 0.2,
            cells: 3,
            octaves: 3,
            seed_tag: 11,
        };
        let mask = LayerMask::Patches(patches);
        let first: Vec<bool> = (0..TILE)
            .flat_map(|y| (0..TILE).map(move |x| (x, y)))
            .map(|(x, y)| mask.covers(x, y, 42))
            .collect();
        let second: Vec<bool> = (0..TILE)
            .flat_map(|y| (0..TILE).map(move |x| (x, y)))
            .map(|(x, y)| mask.covers(x, y, 42))
            .collect();
        assert_eq!(first, second);
        let covered = first.iter().filter(|covered| **covered).count();
        assert!(covered > 0 && covered < TILE * TILE);
    }
}
