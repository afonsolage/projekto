//! World generation settings, loaded from `assets/worldgen.ron`.

use std::{fs::File, path::Path};

use projekto_core::{coords::Chunk, voxel::Kind};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Top-level world generation configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorldGenConfig {
    /// Seed for every noise field. Changing it changes the whole world.
    pub seed: u64,
    /// Y coordinate of the future water level. The terrain floor stays above it until water
    /// rendering lands.
    pub sea_level: i32,
    /// Lowest generated surface height.
    pub min_height: i32,
    /// Highest generated surface height, including peaks.
    pub max_height: i32,
    /// 2D shape fields.
    pub fields: FieldSet,
    /// Maps continentalness to base elevation.
    pub base_height: SplineConfig,
    /// Maps erosion to the peaks-and-valleys amplitude.
    pub relief_amplitude: SplineConfig,
    /// Amplitude of the small high-frequency height detail.
    pub detail_strength: f32,
    /// Surface material rules.
    pub surface: SurfaceConfig,
    /// Caves and overhangs.
    pub caves: CaveConfig,
}

/// The 2D noise fields used by the height model.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FieldSet {
    pub continentalness: FieldConfig,
    pub erosion: FieldConfig,
    pub weirdness: FieldConfig,
    pub detail: FieldConfig,
}

/// One fractal noise field.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct FieldConfig {
    pub frequency: f32,
    pub octaves: u8,
    pub gain: f32,
    pub lacunarity: f32,
}

/// A monotone cubic spline over sorted control points.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SplineConfig {
    pub points: Vec<(f32, f32)>,
}

/// Which kinds go on and under the surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceConfig {
    /// Slopes above this value expose `steep` instead of `top` and `subsurface`.
    pub max_slope: f32,
    /// Kind on flat ground above the water line.
    pub top: String,
    /// Kind directly under `top`.
    pub subsurface: String,
    /// How many voxels of `subsurface` follow `top`.
    pub subsurface_depth: u8,
    /// Kind on steep slopes and cliffs.
    pub steep: String,
    /// Kind for everything below the surface layers.
    pub deep: String,
}

/// Caves and overhangs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CaveConfig {
    pub enabled: bool,
    /// 3D field that lifts or lowers the surface.
    pub overhang: FieldConfig,
    /// How many blocks the overhang field can move the surface.
    pub overhang_strength: f32,
    /// 3D field carved out as caverns.
    pub cavern: FieldConfig,
    /// Voxels where the cavern field is above this value become air.
    pub cavern_threshold: f32,
    /// Vertical distance around the surface where 3D fields are sampled.
    pub band: i32,
    /// Horizontal lattice stride. Must divide the chunk width.
    pub cell_xz: u8,
    /// Vertical lattice stride. Must divide the chunk height.
    pub cell_y: u8,
}

/// Errors raised while loading or validating [`WorldGenConfig`].
#[derive(Debug, Error)]
pub enum WorldGenConfigError {
    #[error("failed to read world generation config at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse world generation config at {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: ron::error::SpannedError,
    },
    #[error("invalid world generation config: {0}")]
    Invalid(String),
}

impl WorldGenConfig {
    /// Loads `worldgen.ron` next to the other assets. Panics with the file path on failure.
    pub fn load_default() -> Self {
        let path = format!("{}/worldgen.ron", env!("ASSETS_PATH"));
        Self::load(&path).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Loads and validates the configuration file at `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, WorldGenConfigError> {
        let path = path.as_ref();
        let display = path.display().to_string();
        let file = File::open(path).map_err(|source| WorldGenConfigError::Io {
            path: display.clone(),
            source,
        })?;
        let config: Self =
            ron::de::from_reader(file).map_err(|source| WorldGenConfigError::Parse {
                path: display,
                source,
            })?;

        config.validate()?;
        Ok(config)
    }

    /// Rejects configurations that would panic or produce out-of-bounds heights later.
    pub fn validate(&self) -> Result<(), WorldGenConfigError> {
        let max_y = Chunk::Y_AXIS_SIZE as i32;

        if self.min_height < 0 {
            return Err(invalid(format!(
                "min_height must be at least 0, got {}",
                self.min_height
            )));
        }
        if self.max_height >= max_y {
            return Err(invalid(format!(
                "max_height must be below {max_y}, got {}",
                self.max_height
            )));
        }
        if self.min_height >= self.max_height {
            return Err(invalid(format!(
                "min_height ({}) must be below max_height ({})",
                self.min_height, self.max_height
            )));
        }
        if !(0..max_y).contains(&self.sea_level) {
            return Err(invalid(format!(
                "sea_level must be in 0..{max_y}, got {}",
                self.sea_level
            )));
        }

        validate_field("fields.continentalness", &self.fields.continentalness)?;
        validate_field("fields.erosion", &self.fields.erosion)?;
        validate_field("fields.weirdness", &self.fields.weirdness)?;
        validate_field("fields.detail", &self.fields.detail)?;
        validate_spline("base_height", &self.base_height)?;
        validate_spline("relief_amplitude", &self.relief_amplitude)?;

        if !self.detail_strength.is_finite() || self.detail_strength < 0.0 {
            return Err(invalid(format!(
                "detail_strength must be finite and non-negative, got {}",
                self.detail_strength
            )));
        }

        if !self.surface.max_slope.is_finite() || self.surface.max_slope < 0.0 {
            return Err(invalid(format!(
                "surface.max_slope must be finite and non-negative, got {}",
                self.surface.max_slope
            )));
        }
        if self.surface.subsurface_depth > 16 {
            return Err(invalid(format!(
                "surface.subsurface_depth must be at most 16, got {}",
                self.surface.subsurface_depth
            )));
        }
        for (field, name) in [
            ("surface.top", &self.surface.top),
            ("surface.subsurface", &self.surface.subsurface),
            ("surface.steep", &self.surface.steep),
            ("surface.deep", &self.surface.deep),
        ] {
            if Kind::from_name(name).is_none() {
                return Err(invalid(format!("{field} references unknown kind {name:?}")));
            }
        }

        if self.caves.enabled {
            validate_field("caves.overhang", &self.caves.overhang)?;
            validate_field("caves.cavern", &self.caves.cavern)?;

            if self.caves.band < 0 {
                return Err(invalid(format!(
                    "caves.band must be non-negative, got {}",
                    self.caves.band
                )));
            }
            if self.caves.cell_xz == 0
                || !(Chunk::X_AXIS_SIZE as u8).is_multiple_of(self.caves.cell_xz)
            {
                return Err(invalid(format!(
                    "caves.cell_xz must divide {}, got {}",
                    Chunk::X_AXIS_SIZE,
                    self.caves.cell_xz
                )));
            }
            if self.caves.cell_y == 0
                || !(Chunk::Y_AXIS_SIZE as u16).is_multiple_of(self.caves.cell_y as u16)
            {
                return Err(invalid(format!(
                    "caves.cell_y must divide {}, got {}",
                    Chunk::Y_AXIS_SIZE,
                    self.caves.cell_y
                )));
            }
            if !self.caves.cavern_threshold.is_finite()
                || !(-1.0..=1.0).contains(&self.caves.cavern_threshold)
            {
                return Err(invalid(format!(
                    "caves.cavern_threshold must be in -1..=1, got {}",
                    self.caves.cavern_threshold
                )));
            }
            if !self.caves.overhang_strength.is_finite() || self.caves.overhang_strength < 0.0 {
                return Err(invalid(format!(
                    "caves.overhang_strength must be finite and non-negative, got {}",
                    self.caves.overhang_strength
                )));
            }
        }

        Ok(())
    }
}

fn validate_field(path: &str, field: &FieldConfig) -> Result<(), WorldGenConfigError> {
    if !field.frequency.is_finite() || field.frequency <= 0.0 {
        return Err(invalid(format!(
            "{path}.frequency must be finite and positive, got {}",
            field.frequency
        )));
    }
    if field.octaves == 0 {
        return Err(invalid(format!("{path}.octaves must be at least 1")));
    }
    if !field.gain.is_finite() || field.gain <= 0.0 || field.gain > 1.0 {
        return Err(invalid(format!(
            "{path}.gain must be in (0, 1], got {}",
            field.gain
        )));
    }
    if !field.lacunarity.is_finite() || field.lacunarity <= 0.0 {
        return Err(invalid(format!(
            "{path}.lacunarity must be finite and positive, got {}",
            field.lacunarity
        )));
    }

    Ok(())
}

fn validate_spline(path: &str, spline: &SplineConfig) -> Result<(), WorldGenConfigError> {
    if spline.points.len() < 2 {
        return Err(invalid(format!(
            "{path} must have at least two points, got {}",
            spline.points.len()
        )));
    }

    let mut previous = f32::NEG_INFINITY;
    for (index, &(x, y)) in spline.points.iter().enumerate() {
        if !x.is_finite() || !y.is_finite() {
            return Err(invalid(format!(
                "{path}.points[{index}] must be finite, got ({x}, {y})"
            )));
        }
        if x <= previous {
            return Err(invalid(format!(
                "{path}.points must be sorted by ascending x; point {index} has x={x} after {previous}"
            )));
        }
        previous = x;
    }

    Ok(())
}

fn invalid(message: String) -> WorldGenConfigError {
    WorldGenConfigError::Invalid(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_config_is_valid() {
        WorldGenConfig::load_default();
    }

    #[test]
    fn rejects_unknown_surface_kind() {
        let mut config = WorldGenConfig::load_default();
        config.surface.top = "NotAKind".to_string();

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_unsorted_spline() {
        let mut config = WorldGenConfig::load_default();
        config.base_height.points.swap(0, 1);

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_zero_frequency() {
        let mut config = WorldGenConfig::load_default();
        config.fields.erosion.frequency = 0.0;

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_out_of_range_sea_level() {
        let mut config = WorldGenConfig::load_default();
        config.sea_level = Chunk::Y_AXIS_SIZE as i32;

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_bad_lattice_cell() {
        let mut config = WorldGenConfig::load_default();
        config.caves.cell_xz = 3;

        assert!(config.validate().is_err());
    }
}
