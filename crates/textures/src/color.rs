//! sRGB, Oklab and Oklch helpers used to build deterministic tile palettes.
//!
//! Palettes are built in Oklch and stored as sRGB bytes. Gamut mapping reduces chroma until the
//! color fits sRGB instead of clipping channels, which would shift the hue of saturated ramps.
//!
//! Tile generators write `GEN_LEVELS` palette indices. An art preset decides how many actual
//! colors those indices map to: more colors give a soft painterly ramp, fewer colors give a
//! crisp pixel-art look. The mapping is done here so generators stay art-agnostic.
//!
//! Palette conversion uses `f32` transcendental intrinsics (`sin`, `cos`, `powf`), so a seed is
//! deterministic for a given platform and toolchain but is not guaranteed to be bit-identical
//! across platforms. Any cross-platform difference is at most a few sRGB steps, which is cosmetic
//! for terrain tiles.
//!
//! References: <https://bottosson.github.io/posts/oklab/>.

use crate::hash::hash2d;

/// Palette indices written by the tile generators. Index 0 is the darkest step.
pub const GEN_LEVELS: usize = 6;

/// Per-step lightness offsets around the base color, in Oklab lightness units.
const LEVEL_LIGHTNESS: [f32; GEN_LEVELS] = [-0.150, -0.085, -0.025, 0.025, 0.085, 0.150];
/// Per-step hue offsets in degrees. Shadows lean cool and highlights lean warm.
const LEVEL_HUE: [f32; GEN_LEVELS] = [-9.0, -5.0, -2.0, 2.0, 5.0, 9.0];
/// Per-step chroma multipliers. Extremes are slightly desaturated so ramps read as one material.
const LEVEL_CHROMA: [f32; GEN_LEVELS] = [0.82, 0.92, 0.98, 1.0, 0.96, 0.86];

/// A color in Oklab's polar form: lightness, chroma and hue in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

impl Oklch {
    pub const fn new(l: f32, c: f32, h: f32) -> Self {
        Self { l, c, h }
    }

    /// Converts to sRGB bytes, reducing chroma until the color fits the sRGB gamut.
    pub fn to_srgb8(self) -> [u8; 3] {
        let mut chroma = self.c;
        for _ in 0..16 {
            let rgb = self.with_chroma(chroma).to_linear_rgb();
            if rgb.iter().all(|channel| (0.0..=1.0).contains(channel)) {
                return linear_rgb_to_srgb8(rgb);
            }
            chroma *= 0.85;
        }
        linear_rgb_to_srgb8(self.with_chroma(0.0).to_linear_rgb())
    }

    fn with_chroma(self, chroma: f32) -> Self {
        Self { c: chroma, ..self }
    }

    fn to_linear_rgb(self) -> [f32; 3] {
        let hue = self.h.to_radians();
        let a = self.c * hue.cos();
        let b = self.c * hue.sin();

        let l_ = self.l + 0.396_337_78 * a + 0.215_803_76 * b;
        let m_ = self.l - 0.105_561_346 * a - 0.063_854_17 * b;
        let s_ = self.l - 0.089_484_18 * a - 1.291_485_5 * b;

        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;

        [
            4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
            -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
            -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
        ]
    }
}

fn linear_rgb_to_srgb8(rgb: [f32; 3]) -> [u8; 3] {
    [
        linear_to_srgb(rgb[0].clamp(0.0, 1.0)),
        linear_to_srgb(rgb[1].clamp(0.0, 1.0)),
        linear_to_srgb(rgb[2].clamp(0.0, 1.0)),
    ]
}

fn linear_to_srgb(value: f32) -> u8 {
    let srgb = if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_hue(a: f32, b: f32, t: f32) -> f32 {
    let delta = (b - a).rem_euclid(360.0);
    let delta = if delta > 180.0 { delta - 360.0 } else { delta };
    a + delta * t
}

fn lerp_oklch(a: Oklch, b: Oklch, t: f32) -> Oklch {
    Oklch::new(lerp(a.l, b.l, t), lerp(a.c, b.c, t), lerp_hue(a.h, b.h, t))
}

/// Art-direction transform applied to every palette of a tileset.
#[derive(Clone, Copy, Debug)]
pub struct PaletteTransform {
    /// Multiplier for the lightness spread of the ramp.
    pub contrast: f32,
    /// Multiplier for chroma.
    pub chroma: f32,
    /// Positive values shift hues toward warm (orange) and brighten highlights.
    pub temperature: f32,
    /// Extra lightness added to the top of the ramp.
    pub highlight_boost: f32,
}

impl Default for PaletteTransform {
    fn default() -> Self {
        Self {
            contrast: 1.0,
            chroma: 1.0,
            temperature: 0.0,
            highlight_boost: 0.0,
        }
    }
}

/// A per-tile color ramp, ordered from darkest to lightest.
///
/// `colors` holds the actual art-directed shades and `mapped` the six colors the tile generators
/// resolve through, so a ramp with four or twelve shades still maps a generated level with one
/// table lookup.
#[derive(Clone, Debug)]
pub struct Ramp {
    pub colors: Vec<Oklch>,
    mapped: [[u8; 3]; GEN_LEVELS],
}

/// Small seeded perturbation applied on top of the authored palette.
#[derive(Clone, Copy, Debug, Default)]
pub struct PaletteJitter {
    pub lightness: f32,
    pub chroma: f32,
    pub hue: f32,
}

impl PaletteJitter {
    /// Derives a palette jitter from a material tag and a world seed.
    pub fn from_seed(seed: u64, tag: u64) -> Self {
        let hash = hash2d(0, tag as i32, seed);
        let lightness = (hash & 0xFF) as f32 / 255.0 - 0.5;
        let chroma = ((hash >> 8) & 0xFF) as f32 / 255.0 - 0.5;
        let hue = ((hash >> 16) & 0xFF) as f32 / 255.0 - 0.5;
        Self {
            lightness: lightness * 0.030,
            chroma: chroma * 0.14,
            hue: hue * 7.0,
        }
    }
}

impl Ramp {
    /// Builds a ramp around `base` with `levels` shades, spread by `spread` in each direction.
    pub fn new(
        base: Oklch,
        spread: f32,
        levels: usize,
        transform: PaletteTransform,
        jitter: PaletteJitter,
    ) -> Self {
        let levels = levels.clamp(2, 12);
        let mut colors = Vec::with_capacity(levels);

        for index in 0..levels {
            let t = index as f32 / (levels - 1) as f32;
            let (profile_lightness, profile_hue, profile_chroma) = sample_profile(t);
            let highlight = ((t - 0.30) / 0.70).max(0.0);

            let lightness = (base.l
                + profile_lightness * spread * transform.contrast
                + jitter.lightness
                + transform.highlight_boost * highlight * highlight)
                .clamp(0.02, 0.98);
            let chroma =
                (base.c * profile_chroma * transform.chroma * (1.0 + jitter.chroma)).max(0.0);
            let hue = base.h + profile_hue * 0.6 + jitter.hue - transform.temperature * 9.0;

            colors.push(Oklch::new(lightness, chroma, hue));
        }

        let mapped = std::array::from_fn(|index| {
            let position = index as f32 / (GEN_LEVELS - 1) as f32 * (levels - 1) as f32;
            let low = position.floor() as usize;
            let high = (low + 1).min(levels - 1);
            let fraction = position - low as f32;
            if low == high {
                colors[low].to_srgb8()
            } else {
                lerp_oklch(colors[low], colors[high], fraction).to_srgb8()
            }
        });

        Self { colors, mapped }
    }

    /// Returns the sRGB bytes a tile generator level resolves to.
    pub fn srgb_for_index(&self, index: u8) -> [u8; 3] {
        self.mapped[(index as usize).min(GEN_LEVELS - 1)]
    }

    /// Number of actual shades in the ramp.
    pub fn levels(&self) -> usize {
        self.colors.len()
    }
}

/// Samples the authored six-point palette profile at `t` in `0..=1`.
fn sample_profile(t: f32) -> (f32, f32, f32) {
    let position = t.clamp(0.0, 1.0) * (GEN_LEVELS - 1) as f32;
    let low = position.floor() as usize;
    let high = (low + 1).min(GEN_LEVELS - 1);
    let fraction = position - low as f32;
    (
        lerp(
            LEVEL_LIGHTNESS[low] / 0.150,
            LEVEL_LIGHTNESS[high] / 0.150,
            fraction,
        ),
        lerp(LEVEL_HUE[low], LEVEL_HUE[high], fraction),
        lerp(LEVEL_CHROMA[low], LEVEL_CHROMA[high], fraction),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(levels: usize) -> Ramp {
        Ramp::new(
            Oklch::new(0.5, 0.1, 60.0),
            0.12,
            levels,
            PaletteTransform::default(),
            PaletteJitter::default(),
        )
    }

    #[test]
    fn ramp_is_ordered_dark_to_light() {
        let ramp = ramp(GEN_LEVELS);
        for pair in ramp.colors.windows(2) {
            assert!(pair[0].l <= pair[1].l);
        }
    }

    #[test]
    fn mapped_indices_grow_brighter() {
        let ramp = ramp(6);
        let mut previous = 0;
        for index in 0..GEN_LEVELS as u8 {
            let brightness: u32 = ramp.srgb_for_index(index).iter().map(|c| *c as u32).sum();
            assert!(brightness >= previous || index == 0);
            previous = brightness;
        }
    }

    #[test]
    fn presets_control_ramp_resolution() {
        assert_eq!(ramp(3).mapped.len(), GEN_LEVELS);
        assert_eq!(ramp(12).levels(), 12);
        assert_eq!(ramp(6).levels(), GEN_LEVELS);
    }

    #[test]
    fn out_of_gamut_colors_still_produce_bytes() {
        let color = Oklch::new(0.5, 0.9, 30.0);
        let [r, g, b] = color.to_srgb8();
        assert!(r as u32 + g as u32 + b as u32 > 0);
    }

    #[test]
    fn jitter_is_deterministic() {
        let a = PaletteJitter::from_seed(42, 3);
        let b = PaletteJitter::from_seed(42, 3);
        let c = PaletteJitter::from_seed(43, 3);
        assert_eq!(a.hue, b.hue);
        assert_ne!(a.hue, c.hue);
    }
}
