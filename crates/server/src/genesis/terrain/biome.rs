//! Biome selection and the resolved per-biome surface table.

use projekto_core::voxel::Kind;

use super::{
    config::{BiomeParameters, BiomesConfig},
    surface::SurfaceRule,
};

/// Nearest-point biome table in `(temperature, humidity, erosion)` space.
pub(crate) struct BiomeTable {
    entries: Vec<BiomeEntry>,
    max_slope: f32,
    steep: Kind,
    deep: Kind,
}

struct BiomeEntry {
    parameters: BiomeParameters,
    surface: SurfaceRule,
}

impl BiomeTable {
    pub(crate) fn new(config: &BiomesConfig) -> Self {
        let entries = config
            .entries
            .iter()
            .map(|biome| BiomeEntry {
                parameters: biome.parameters,
                surface: SurfaceRule::new(&biome.surface),
            })
            .collect();

        Self {
            entries,
            max_slope: config.max_slope,
            steep: resolve(&config.steep),
            deep: resolve(&config.deep),
        }
    }

    /// Returns the index of the nearest biome entry.
    pub(crate) fn select(&self, temperature: f32, humidity: f32, erosion: f32) -> usize {
        let mut best = 0;
        let mut best_distance = f32::MAX;

        for (index, entry) in self.entries.iter().enumerate() {
            let parameters = &entry.parameters;
            let dt = temperature - parameters.temperature;
            let dh = humidity - parameters.humidity;
            let de = erosion - parameters.erosion;
            let distance = dt * dt + dh * dh + de * de;

            if distance < best_distance {
                best = index;
                best_distance = distance;
            }
        }

        best
    }

    /// Kind at `depth` below a column's top for `biome`.
    pub(crate) fn surface(&self, biome: usize, depth: i32, slope: f32) -> Kind {
        self.entries[biome]
            .surface
            .kind(depth, slope, self.max_slope, self.steep, self.deep)
    }

    /// Number of subsurface voxels for `biome`.
    pub(crate) fn surface_depth(&self, biome: usize) -> i32 {
        self.entries[biome].surface.depth()
    }

    /// Kind used for everything below the surface layers.
    pub(crate) fn deep(&self) -> Kind {
        self.deep
    }
}

fn resolve(name: &str) -> Kind {
    Kind::from_name(name).unwrap_or_else(|| panic!("biome references unknown kind {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::WorldGenConfig;

    fn table() -> (WorldGenConfig, BiomeTable) {
        let config = WorldGenConfig::load_default();
        let table = BiomeTable::new(&config.biomes);

        (config, table)
    }

    #[test]
    fn each_biome_selects_its_own_point() {
        let (config, table) = table();

        for (index, biome) in config.biomes.entries.iter().enumerate() {
            let parameters = biome.parameters;
            assert_eq!(
                table.select(
                    parameters.temperature,
                    parameters.humidity,
                    parameters.erosion
                ),
                index,
                "biome {} did not select its own point",
                biome.name
            );
        }
    }

    #[test]
    fn selection_is_deterministic_for_extreme_values() {
        let (config, table) = table();

        for step in 0..256 {
            let value = step as f32 * 0.31 - 40.0;
            let first = table.select(value, -value, value * 0.5);
            let second = table.select(value, -value, value * 0.5);

            assert_eq!(first, second);
            assert!(first < config.biomes.entries.len());
        }
    }
}
