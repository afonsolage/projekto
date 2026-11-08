//! Structural tile synthesis: periodic noise fields plus material-specific motifs.
//!
//! Two detail levels share the same code: [`Detail::Noise`] is the cheap baseline (fractal noise
//! mapped through a ramp) and [`Detail::Full`] adds the motifs that give each material its
//! identity: tufts for grass, cracks and strata for rock, ripples for sand, pebbles for dirt,
//! lichen for moss and sparkles for snow.
//!
//! Every motif is scaled by [`ArtParameters`] so art presets can go from barely-there detail to
//! strong, crisp pixel-art relief without touching the generators.

use std::f32::consts::TAU;

use crate::art::ArtParameters;
use crate::color::GEN_LEVELS;
use crate::hash::{derive_seed, hash2d, hash2d_unit};
use crate::index_tile::{IndexTile, MARGIN, TILE, TILE_PIXELS, TileContent};
use crate::layers::{LayerMask, apply_patches, apply_top_band};
use crate::materials::{Material, MaterialKind};
use crate::noise::{PeriodicNoise, PeriodicWorley};

const TAG_DIRT: u64 = 0x11;
const TAG_GRAIN: u64 = 0x12;
const TAG_PEBBLE: u64 = 0x13;
const TAG_GRASS: u64 = 0x21;
const TAG_TUFT: u64 = 0x22;
const TAG_SPECK: u64 = 0x23;
const TAG_ROCK: u64 = 0x31;
const TAG_FACET: u64 = 0x32;
const TAG_STRATA: u64 = 0x33;
const TAG_CRACK: u64 = 0x34;
const TAG_SAND: u64 = 0x41;
const TAG_RIPPLE: u64 = 0x42;
const TAG_SPARKLE: u64 = 0x43;
const TAG_SNOW: u64 = 0x51;
const TAG_MOSS: u64 = 0x61;
const TAG_LICHEN: u64 = 0x62;
const TAG_LAMP: u64 = 0x71;

/// How many structure layers a style synthesizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// Fractal noise only, with the minimum structure a material needs to read correctly.
    Noise,
    /// Full layered synthesis with material motifs.
    Full,
}

/// Generates the final content for a material, simple or composite.
pub fn tile(material: &Material, seed: u64, detail: Detail, art: &ArtParameters) -> TileContent {
    if material.is_composite() {
        composite(material, seed, detail, art)
    } else {
        TileContent::Indexed(surface(material.kind, seed, detail, art))
    }
}

/// Generates the pattern of a simple material from scratch.
///
/// Composite materials are assembled by [`composite`] from simple sources, so this function only
/// handles the source materials.
pub fn surface(kind: MaterialKind, seed: u64, detail: Detail, art: &ArtParameters) -> IndexTile {
    match kind {
        MaterialKind::Dirt => dirt(seed, detail, art),
        MaterialKind::GrassTop => grass_top(seed, detail, art),
        MaterialKind::Rock => rock(seed, detail, art),
        MaterialKind::Sand => sand(seed, detail, art),
        MaterialKind::Snow => snow(seed, detail, art),
        MaterialKind::Moss => moss(seed, detail, art),
        MaterialKind::Lamp => lamp(seed),
        MaterialKind::GrassSide
        | MaterialKind::SnowSide
        | MaterialKind::MossSide
        | MaterialKind::MossyRock
        | MaterialKind::SnowyGrassSide => {
            debug_assert!(false, "{kind:?} is composite and has no single pattern");
            dirt(seed, detail, art)
        }
    }
}

/// Stacks the material's layer patterns through their procedural masks.
///
/// The first layer is the full base; every following layer reuses a simple material's pattern and
/// palette and draws only where its mask covers. Cross-referencing the source material is what
/// keeps a grass side's turf identical to the grass top's turf without an authored mask.
pub fn composite(
    material: &Material,
    seed: u64,
    detail: Detail,
    art: &ArtParameters,
) -> TileContent {
    let layers = material.layers;
    debug_assert!(!layers.is_empty(), "composite needs at least one layer");

    let mut levels = surface(layers[0].source, seed, detail, art);
    let mut palette = [0u8; TILE_PIXELS];

    for (index, layer) in layers.iter().enumerate().skip(1) {
        let overlay = surface(layer.source, seed, detail, art);
        let overlay_index = index as u8;

        match layer.mask {
            LayerMask::Full => {
                levels = overlay;
                palette = [overlay_index; TILE_PIXELS];
            }
            LayerMask::TopBand(band) => {
                apply_top_band(
                    &mut levels,
                    &mut palette,
                    &overlay,
                    &band,
                    overlay_index,
                    seed,
                );
            }
            LayerMask::Patches(patches) => {
                apply_patches(
                    &mut levels,
                    &mut palette,
                    &overlay,
                    &patches,
                    overlay_index,
                    seed,
                );
            }
        }
    }

    TileContent::layered(levels, palette)
}

fn uv(x: usize, y: usize) -> (f32, f32) {
    (
        (x as f32 + 0.5) / TILE as f32,
        (y as f32 + 0.5) / TILE as f32,
    )
}

fn bevel_strength(detail: Detail, full: f32, noise: f32) -> f32 {
    match detail {
        Detail::Noise => noise,
        Detail::Full => full,
    }
}

// -- dirt -----------------------------------------------------------------------------------

fn dirt(seed: u64, detail: Detail, art: &ArtParameters) -> IndexTile {
    let base = PeriodicNoise::new(derive_seed(seed, TAG_DIRT), 4);
    let mut tile = IndexTile::filled(3);

    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            let level = 3.0 + base.fbm(u, v, 4) * 1.5;
            tile.set(x, y, level.round() as i32);
        }
    }

    if detail == Detail::Full {
        speckle(
            &mut tile,
            seed,
            TAG_GRAIN,
            Speckle::new(0.055, -2, 0.02, 1),
            art,
        );
        pebbles(&mut tile, seed, art);
    }

    tile.bevel(bevel_strength(detail, 0.35, 0.12) * art.bevel);
    tile
}

// -- grass ----------------------------------------------------------------------------------

fn grass_top(seed: u64, detail: Detail, art: &ArtParameters) -> IndexTile {
    let base = PeriodicNoise::new(derive_seed(seed, TAG_GRASS), 4);
    let clumps = PeriodicNoise::new(derive_seed(seed, TAG_GRASS), 2);
    let mut tile = IndexTile::filled(2);

    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            // A low-frequency term makes patches of grass lighter or darker instead of a flat film.
            let level = 2.2 + base.fbm(u, v, 4) * 1.4 + clumps.fbm(u, v, 2) * 0.5;
            tile.set(x, y, level.round() as i32);
        }
    }

    if detail == Detail::Full {
        tufts(&mut tile, seed, art);
        speckle(
            &mut tile,
            seed,
            TAG_SPECK,
            Speckle::new(0.05, -1, 0.02, 1),
            art,
        );
    }

    tile.bevel(bevel_strength(detail, 0.2, 0.08) * art.bevel);
    tile
}

/// Places light blades on a jittered 4x4 grid so the grass tile reads as tufts, not noise.
fn tufts(tile: &mut IndexTile, seed: u64, art: &ArtParameters) {
    let tuft_seed = derive_seed(seed, TAG_TUFT);
    let count = (10.0 * art.structure).round().clamp(2.0, 16.0) as usize;
    let threshold = (62.0 * art.structure).clamp(10.0, 96.0);
    let mut placed = 0;

    for grid_y in 0..4 {
        for grid_x in 0..4 {
            if placed >= count {
                return;
            }
            let hash = hash2d(grid_x, grid_y, tuft_seed);
            if (hash % 100) as f32 >= threshold {
                continue;
            }
            let x = MARGIN + 1 + grid_x as usize * 7 + (hash >> 8) as usize % 4;
            let y = MARGIN + 3 + grid_y as usize * 6 + (hash >> 16) as usize % 4;
            let cluster = 1 + ((hash >> 20) % 3) as usize;
            for blade in 0..cluster {
                let blade_x = x + blade * 2;
                if blade_x >= TILE - MARGIN - 1 {
                    break;
                }
                let height = 2 + ((hash >> (23 + blade)) & 1) as usize;
                draw_blade(tile, blade_x, y, height, (hash >> 5) & 1 == 0);
            }
            placed += 1;
        }
    }
}

/// Draws a two to four pixel tall blade with a bright tip and a dark root.
fn draw_blade(tile: &mut IndexTile, x: usize, y: usize, height: usize, lean: bool) {
    if !(MARGIN + 1..TILE - MARGIN - 1).contains(&x) || y < MARGIN + height + 1 {
        return;
    }

    let base_level = tile.get(x, y) as i32;
    tile.set(x, y, (base_level - 2).max(0));

    let tip = (GEN_LEVELS as i32 - 1).min(base_level + 3);
    let mid = (tip - 1).max(1);

    for step in 1..=height {
        let row = y - step;
        let column = if lean && step + 1 >= height { x + 1 } else { x };
        if !(MARGIN..TILE - MARGIN).contains(&column) {
            continue;
        }
        let level = if step == height { tip } else { mid };
        tile.set(column, row, level);
    }
}

// -- rock -----------------------------------------------------------------------------------

fn rock(seed: u64, detail: Detail, art: &ArtParameters) -> IndexTile {
    let base = PeriodicNoise::new(derive_seed(seed, TAG_ROCK), 4);
    let mut tile = IndexTile::filled(3);

    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            tile.set(x, y, (2.9 + base.fbm(u, v, 4) * 1.2).round() as i32);
        }
    }

    if detail == Detail::Full {
        let facets = PeriodicWorley::new(derive_seed(seed, TAG_FACET), 4);
        let structure = art.structure.clamp(0.3, 1.6);
        for y in 0..TILE {
            for x in 0..TILE {
                let (u, v) = uv(x, y);
                let sample = facets.sample(u, v);
                let lift = (sample.cell % 2) as f32 - 0.5;
                let mut level = tile.get(x, y) as f32 + lift * 0.6 * structure;
                level += (0.5 - sample.f1 * 0.5) * 0.3 * structure;
                if sample.f2 - sample.f1 < 0.035 {
                    level = (level - 2.0).max(0.0);
                }
                tile.set(x, y, level.round() as i32);
            }
        }

        strata(&mut tile, seed, 0.35 * art.structure);
        speckle(
            &mut tile,
            seed,
            TAG_CRACK,
            Speckle::new(0.02, -1, 0.015, 1),
            art,
        );
    }

    tile.bevel(bevel_strength(detail, 0.4, 0.12) * art.bevel);
    tile
}

fn strata(tile: &mut IndexTile, seed: u64, strength: f32) {
    let noise = PeriodicNoise::new(derive_seed(seed, TAG_STRATA), 2);
    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            // Four full cycles per tile keep the pattern periodic across the border.
            let phase = v * 4.0 + noise.fbm_1d(u, 2) * 0.35;
            let wave = (phase * TAU).sin();
            let mut level = tile.get(x, y) as f32 + wave * strength;
            if wave < -0.9 {
                level -= 1.0;
            }
            tile.set(x, y, level.round() as i32);
        }
    }
}

// -- sand -----------------------------------------------------------------------------------

fn sand(seed: u64, detail: Detail, art: &ArtParameters) -> IndexTile {
    let base = PeriodicNoise::new(derive_seed(seed, TAG_SAND), 6);
    let mut tile = IndexTile::filled(3);

    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            tile.set(x, y, (3.0 + base.fbm(u, v, 4) * 1.1).round() as i32);
        }
    }

    if detail == Detail::Full {
        ripples(&mut tile, seed, 0.3 * art.structure);
        speckle(
            &mut tile,
            seed,
            TAG_SPARKLE,
            Speckle::new(0.02, -1, 0.015, 2),
            art,
        );
    }

    tile.bevel(bevel_strength(detail, 0.14, 0.08) * art.bevel);
    tile
}

fn ripples(tile: &mut IndexTile, seed: u64, strength: f32) {
    let noise = PeriodicNoise::new(derive_seed(seed, TAG_RIPPLE), 2);
    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            let phase = v * 3.0 + noise.fbm(u, v, 2) * 0.55;
            let wave = (phase * TAU).sin();
            let mut level = tile.get(x, y) as f32 + wave * strength;
            if wave < -0.82 {
                level -= 0.9;
            }
            tile.set(x, y, level.round() as i32);
        }
    }
}

// -- snow -----------------------------------------------------------------------------------

fn snow(seed: u64, detail: Detail, art: &ArtParameters) -> IndexTile {
    let base = PeriodicNoise::new(derive_seed(seed, TAG_SNOW), 3);
    let mut tile = IndexTile::filled(4);

    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            tile.set(x, y, (3.4 + base.fbm(u, v, 4) * 1.0).round() as i32);
        }
    }

    if detail == Detail::Full {
        hollows(&mut tile, seed, 0.9 * art.structure);
        speckle(
            &mut tile,
            seed,
            TAG_SPARKLE,
            Speckle::new(0.01, -1, 0.015, 2),
            art,
        );
    }

    tile.bevel(bevel_strength(detail, 0.1, 0.05) * art.bevel);
    tile
}

fn hollows(tile: &mut IndexTile, seed: u64, strength: f32) {
    let base = PeriodicNoise::new(derive_seed(seed, TAG_SNOW), 3);
    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            let value = base.fbm(u, v, 3);
            let mut level = tile.get(x, y) as f32;
            if value < -0.3 {
                level -= strength;
            } else if value > 0.45 {
                level += strength * 0.5;
            }
            tile.set(x, y, level.round() as i32);
        }
    }
}

// -- moss -----------------------------------------------------------------------------------

fn moss(seed: u64, detail: Detail, art: &ArtParameters) -> IndexTile {
    let base = PeriodicNoise::new(derive_seed(seed, TAG_MOSS), 4);
    let mut tile = IndexTile::filled(2);

    for y in 0..TILE {
        for x in 0..TILE {
            let (u, v) = uv(x, y);
            tile.set(x, y, (2.2 + base.fbm(u, v, 4) * 1.1).round() as i32);
        }
    }

    if detail == Detail::Full {
        lichen(&mut tile, seed, art);
        speckle(
            &mut tile,
            seed,
            TAG_MOSS,
            Speckle::new(0.06, -1, 0.0, 0),
            art,
        );
    }

    tile.bevel(bevel_strength(detail, 0.18, 0.08) * art.bevel);
    tile
}

fn lichen(tile: &mut IndexTile, seed: u64, art: &ArtParameters) {
    let lichen_seed = derive_seed(seed, TAG_LICHEN);
    let threshold = (34.0 * art.structure).clamp(5.0, 90.0);
    for grid_y in 0..7 {
        for grid_x in 0..7 {
            let hash = hash2d(grid_x, grid_y, lichen_seed);
            if (hash % 100) as f32 >= threshold {
                continue;
            }
            let x = MARGIN + grid_x as usize * 4 + (hash >> 8) as usize % 3;
            let y = MARGIN + grid_y as usize * 4 + (hash >> 16) as usize % 3;
            let base = tile.get(x, y) as i32;
            tile.set(x, y, base + 2);
            if hash & 4 != 0 {
                tile.set(x, y.saturating_sub(1), base + 1);
            }
            if hash & 8 != 0 {
                tile.set(x + 1, y, base + 1);
            }
        }
    }
}

// -- lamp -----------------------------------------------------------------------------------

fn lamp(seed: u64) -> IndexTile {
    let mut tile = IndexTile::filled(0);
    let grain = PeriodicNoise::new(derive_seed(seed, TAG_LAMP), 4);

    for quadrant_y in 0..2 {
        for quadrant_x in 0..2 {
            let center_x = 8.0 + quadrant_x as f32 * 16.0;
            let center_y = 8.0 + quadrant_y as f32 * 16.0;

            for y in 0..TILE {
                for x in 0..TILE {
                    let (u, v) = uv(x, y);
                    let dx = (x as f32 + 0.5 - center_x).abs();
                    let dy = (y as f32 + 0.5 - center_y).abs();
                    let distance = dx.max(dy) + grain.fbm(u, v, 2) * 0.35;
                    let level = if distance < 2.0 {
                        5
                    } else if distance < 3.6 {
                        4
                    } else if distance < 4.8 {
                        3
                    } else if distance < 5.2 {
                        1
                    } else {
                        continue;
                    };
                    tile.set(x, y, level);
                }
            }
        }
    }

    tile
}

// -- shared detail layers -------------------------------------------------------------------

/// Speckle probabilities and deltas for one detail layer.
struct Speckle {
    dark_below: f32,
    dark_delta: i32,
    light_above: f32,
    light_delta: i32,
}

impl Speckle {
    const fn new(dark_below: f32, dark_delta: i32, light_above: f32, light_delta: i32) -> Self {
        Self {
            dark_below,
            dark_delta,
            light_above,
            light_delta,
        }
    }
}

/// Adds sparse light or dark pixels: dark if `hash < dark_below`, light if `hash > 1 - light_above`.
///
/// Probabilities are scaled by the art preset's structure amount, so a soft preset only keeps a
/// whisper of grain while a crisp preset gets dense specks.
fn speckle(tile: &mut IndexTile, seed: u64, tag: u64, spec: Speckle, art: &ArtParameters) {
    let speck_seed = derive_seed(seed, tag);
    let dark_below = (spec.dark_below * art.structure).min(0.5);
    let light_above = (spec.light_above * art.structure).min(0.5);
    let Speckle {
        dark_delta,
        light_delta,
        ..
    } = spec;
    for y in MARGIN..TILE - MARGIN {
        for x in MARGIN..TILE - MARGIN {
            let value = hash2d_unit(x as i32, y as i32, speck_seed);
            if value < dark_below {
                tile.add(x, y, dark_delta);
            } else if value > 1.0 - light_above {
                tile.add(x, y, light_delta);
            }
        }
    }
}

/// Places small raised pebbles with a highlight and a shadow edge.
fn pebbles(tile: &mut IndexTile, seed: u64, art: &ArtParameters) {
    let pebble_seed = derive_seed(seed, TAG_PEBBLE);
    let count = (3.0 * art.structure).round().clamp(1.0, 6.0) as usize;
    for index in 0..count {
        let hash = hash2d(index as i32, 0, pebble_seed);
        let x = MARGIN + 1 + (hash as usize % 26);
        let y = MARGIN + 1 + ((hash >> 8) as usize % 26);
        let size = 2 + ((hash >> 16) as usize % 2);
        let base = tile.get(x, y) as i32;

        for dy in 0..size {
            for dx in 0..size {
                tile.set(x + dx, y + dy, base + 1);
            }
        }
        tile.set(x, y, base + 2);
        tile.add(x + size - 1, y + size - 1, -1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::ArtPreset;

    fn art() -> ArtParameters {
        ArtPreset::default().parameters()
    }

    fn material(kind: MaterialKind) -> &'static Material {
        crate::materials::by_kind(kind)
    }

    fn levels_of(content: &TileContent) -> [u32; GEN_LEVELS] {
        match content {
            TileContent::Indexed(tile) | TileContent::Layered { levels: tile, .. } => tile.levels(),
        }
    }

    #[test]
    fn every_material_generates_non_flat_content() {
        for material in &crate::materials::MATERIALS {
            let content = tile(material, 42, Detail::Full, &art());
            let levels = levels_of(&content);
            let used = levels.iter().filter(|&&count| count > 0).count();
            assert!(used >= 2, "{} produced {used} levels", material.name);
        }
    }

    #[test]
    fn composites_use_more_than_one_palette() {
        for material in crate::materials::MATERIALS
            .iter()
            .filter(|m| m.is_composite())
        {
            let content = tile(material, 42, Detail::Full, &art());
            let TileContent::Layered { palette, .. } = &content else {
                panic!("{} must be layered", material.name);
            };
            let mut used = [false; 8];
            for &index in palette.iter() {
                used[index as usize] = true;
            }
            let used = used.iter().filter(|&&used| used).count();
            assert!(used >= 2, "{} used {used} palettes", material.name);
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let first = tile(material(MaterialKind::Rock), 42, Detail::Full, &art());
        let second = tile(material(MaterialKind::Rock), 42, Detail::Full, &art());
        assert_eq!(levels_of(&first), levels_of(&second));
    }

    #[test]
    fn grass_has_light_blades() {
        let content = tile(material(MaterialKind::GrassTop), 42, Detail::Full, &art());
        let levels = levels_of(&content);
        let bright = levels[GEN_LEVELS - 1] + levels[GEN_LEVELS - 2];
        assert!(bright >= 8, "expected visible blades, got {levels:?}");
    }
}
