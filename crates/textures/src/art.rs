//! Art-direction presets on top of a tile synthesis style.
//!
//! A style decides how tiles are built (plain noise or layered structure); an art preset decides
//! how they look: how many shades a tile gets, how saturated and warm the palette is, and how
//! strongly the structural motifs and the relief shading read. Presets are authored data, so
//! choosing a look does not change the generators.

/// Parameters shared by every material of a tileset.
#[derive(Clone, Copy, Debug)]
pub struct ArtParameters {
    pub name: &'static str,
    /// Number of actual palette shades per tile after quantization.
    pub levels: usize,
    /// Palette contrast multiplier.
    pub contrast: f32,
    /// Palette chroma multiplier.
    pub chroma: f32,
    /// Positive values shift the palette toward warm orange and brighten highlights.
    pub temperature: f32,
    /// Lightness added to the brightest end of the ramp.
    pub highlight_boost: f32,
    /// Motif density and strength multiplier for structural layers.
    pub structure: f32,
    /// Relief shading multiplier.
    pub bevel: f32,
}

/// A named art direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ArtPreset {
    /// Soft painterly ramp with light motifs, closest to the current hand-made tiles.
    #[default]
    Soft,
    /// Few shades and strong motifs, for a crisp classic pixel-art look.
    Pixel,
    /// Desaturated, slightly cool and calm.
    Muted,
    /// Warm highlights and warmer palette, for golden-hour terrain.
    Warm,
    /// Saturated and contrasty, for a vivid arcade look.
    Vivid,
    /// Darker, high-contrast and slightly cool, for a dramatic look.
    Dramatic,
}

impl ArtPreset {
    pub const ALL: [Self; 6] = [
        Self::Soft,
        Self::Pixel,
        Self::Muted,
        Self::Warm,
        Self::Vivid,
        Self::Dramatic,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "soft" => Some(Self::Soft),
            "pixel" => Some(Self::Pixel),
            "muted" => Some(Self::Muted),
            "warm" => Some(Self::Warm),
            "vivid" => Some(Self::Vivid),
            "dramatic" => Some(Self::Dramatic),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        self.parameters().name
    }

    /// Returns the authored parameters of the preset.
    pub fn parameters(self) -> ArtParameters {
        match self {
            Self::Soft => ArtParameters {
                name: "soft",
                levels: 12,
                contrast: 0.80,
                chroma: 0.92,
                temperature: 0.05,
                highlight_boost: 0.010,
                structure: 0.5,
                bevel: 0.55,
            },
            Self::Pixel => ArtParameters {
                name: "pixel",
                levels: 4,
                contrast: 1.25,
                chroma: 1.05,
                temperature: 0.10,
                highlight_boost: 0.0,
                structure: 1.0,
                bevel: 1.15,
            },
            Self::Muted => ArtParameters {
                name: "muted",
                levels: 7,
                contrast: 0.88,
                chroma: 0.55,
                temperature: -0.45,
                highlight_boost: 0.0,
                structure: 0.8,
                bevel: 0.75,
            },
            Self::Warm => ArtParameters {
                name: "warm",
                levels: 8,
                contrast: 1.0,
                chroma: 1.10,
                temperature: 1.0,
                highlight_boost: 0.040,
                structure: 0.85,
                bevel: 0.85,
            },
            Self::Vivid => ArtParameters {
                name: "vivid",
                levels: 6,
                contrast: 1.20,
                chroma: 1.55,
                temperature: 0.15,
                highlight_boost: 0.015,
                structure: 1.1,
                bevel: 1.0,
            },
            Self::Dramatic => ArtParameters {
                name: "dramatic",
                levels: 5,
                contrast: 1.45,
                chroma: 0.95,
                temperature: -0.3,
                highlight_boost: 0.02,
                structure: 1.15,
                bevel: 1.3,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_roundtrips_through_its_name() {
        for preset in ArtPreset::ALL {
            assert_eq!(ArtPreset::parse(preset.name()), Some(preset));
        }
    }

    #[test]
    fn presets_differ_in_their_look() {
        let soft = ArtPreset::Soft.parameters();
        let pixel = ArtPreset::Pixel.parameters();
        assert_ne!(soft.levels, pixel.levels);
        assert_ne!(soft.structure, pixel.structure);
    }

    #[test]
    fn unknown_preset_is_rejected() {
        assert_eq!(ArtPreset::parse("comic"), None);
    }
}
