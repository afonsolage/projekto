//! Biome selection and the resolved per-biome surface and shape tables.

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
    transition: f32,
    global_overhang_strength: f32,
}

struct BiomeEntry {
    parameters: BiomeParameters,
    shape: BiomeShape,
    surface: SurfaceRule,
}

#[derive(Clone, Copy)]
struct BiomeShape {
    height_offset: f32,
    relief_multiplier: f32,
    terrace_strength: f32,
    cave_density: f32,
    overhang_strength: f32,
}

/// Shape parameters for one column, blended by selection weight.
#[derive(Clone, Copy)]
pub(crate) struct BlendedShape {
    pub(crate) height_offset: f32,
    pub(crate) relief_multiplier: f32,
    pub(crate) terrace_strength: f32,
    pub(crate) cave_density: f32,
    pub(crate) overhang_strength: f32,
}

impl BiomeTable {
    pub(crate) fn new(config: &BiomesConfig, global_overhang_strength: f32) -> Self {
        let entries = config
            .entries
            .iter()
            .map(|biome| BiomeEntry {
                parameters: biome.parameters,
                shape: BiomeShape {
                    height_offset: biome.shape.height_offset,
                    relief_multiplier: biome.shape.relief_multiplier,
                    terrace_strength: biome.shape.terrace_strength,
                    cave_density: biome.shape.cave_density,
                    overhang_strength: biome.shape.overhang_strength,
                },
                surface: SurfaceRule::new(&biome.surface),
            })
            .collect();

        Self {
            entries,
            max_slope: config.max_slope,
            steep: resolve(&config.steep),
            deep: resolve(&config.deep),
            transition: config.transition,
            global_overhang_strength,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns the index of the nearest biome entry.
    ///
    /// Distances are compared squared, exactly as before blended selection existed, so the
    /// dominant index does not change when selection also returns weights.
    #[cfg(test)]
    pub(crate) fn select(&self, temperature: f32, humidity: f32, erosion: f32) -> usize {
        let mut best = 0;
        let mut best_distance = f32::MAX;

        for (index, entry) in self.entries.iter().enumerate() {
            let distance = squared_distance(&entry.parameters, temperature, humidity, erosion);

            if distance < best_distance {
                best = index;
                best_distance = distance;
            }
        }

        best
    }

    /// Fills `out` with normalized selection weights and returns the dominant index.
    ///
    /// `out` must have one element per biome entry. The dominant index uses the squared-distance
    /// comparison from [`BiomeTable::select`]; the weights use square roots for a distance-based
    /// transition band. A non-finite weight sum falls back to a one-hot dominant weight so extreme
    /// coordinates keep the old nearest-point behavior.
    pub(crate) fn weights_into(
        &self,
        temperature: f32,
        humidity: f32,
        erosion: f32,
        out: &mut [f32],
    ) -> usize {
        debug_assert_eq!(out.len(), self.entries.len());

        let mut best = 0;
        let mut best_distance = f32::MAX;

        for (index, entry) in self.entries.iter().enumerate() {
            let distance = squared_distance(&entry.parameters, temperature, humidity, erosion);
            out[index] = distance;

            if distance < best_distance {
                best = index;
                best_distance = distance;
            }
        }

        let nearest = best_distance.sqrt();
        let mut sum = 0.0;

        for weight in out.iter_mut() {
            let distance = weight.sqrt();
            let falloff = (1.0 - (distance - nearest) / self.transition).max(0.0);
            *weight = falloff * falloff;
            sum += *weight;
        }

        if !sum.is_finite() || sum <= 0.0 {
            out.fill(0.0);
            out[best] = 1.0;
        } else {
            for weight in out.iter_mut() {
                *weight /= sum;
            }
        }

        best
    }

    /// Returns the biome whose surface materials a column uses.
    ///
    /// Inside a transition band several entries have non-zero weight. A deterministic jitter in
    /// `0..1` selects one of them with probability proportional to its weight, so materials
    /// interleave gradually instead of switching at a hard border. Columns where one entry has
    /// full weight always resolve to that entry.
    pub(crate) fn surface_biome(&self, weights: &[f32], jitter: f32) -> usize {
        let mut remaining = jitter.clamp(0.0, 0.999_999);
        let mut dominant = 0;
        let mut dominant_weight = 0.0;

        for (index, &weight) in weights.iter().enumerate() {
            if weight > dominant_weight {
                dominant = index;
                dominant_weight = weight;
            }

            if remaining < weight {
                return index;
            }
            remaining -= weight;
        }

        dominant
    }

    /// Sums the per-biome shape parameters for one column.
    ///
    /// Fields that hold the same value in every entry return that value exactly instead of the
    /// weighted sum. This keeps an all-zero or all-neutral field exact even when the normalized
    /// weights only sum to one within f32 rounding. Non-uniform fields blend multiplier-like
    /// values relative to their neutral value, so a configuration where every entry is neutral
    /// still returns the neutral value.
    pub(crate) fn blended_shape(&self, weights: &[f32]) -> BlendedShape {
        let sum = |field: fn(&BiomeShape) -> f32| {
            self.entries
                .iter()
                .zip(weights)
                .map(|(entry, &weight)| weight * field(&entry.shape))
                .sum::<f32>()
        };
        let uniform = |field: fn(&BiomeShape) -> f32| uniform_shape(&self.entries, field);

        BlendedShape {
            height_offset: uniform(|shape| shape.height_offset)
                .unwrap_or_else(|| sum(|shape| shape.height_offset)),
            relief_multiplier: uniform(|shape| shape.relief_multiplier)
                .unwrap_or_else(|| 1.0 + sum(|shape| shape.relief_multiplier - 1.0)),
            terrace_strength: uniform(|shape| shape.terrace_strength)
                .unwrap_or_else(|| sum(|shape| shape.terrace_strength)),
            cave_density: uniform(|shape| shape.cave_density)
                .unwrap_or_else(|| 1.0 + sum(|shape| shape.cave_density - 1.0)),
            overhang_strength: uniform(|shape| shape.overhang_strength).unwrap_or_else(|| {
                let global = self.global_overhang_strength;
                global
                    + self
                        .entries
                        .iter()
                        .zip(weights)
                        .map(|(entry, &weight)| weight * (entry.shape.overhang_strength - global))
                        .sum::<f32>()
            }),
        }
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

fn uniform_shape(entries: &[BiomeEntry], field: fn(&BiomeShape) -> f32) -> Option<f32> {
    let first = field(&entries[0].shape);

    entries[1..]
        .iter()
        .all(|entry| field(&entry.shape) == first)
        .then_some(first)
}

fn squared_distance(
    parameters: &BiomeParameters,
    temperature: f32,
    humidity: f32,
    erosion: f32,
) -> f32 {
    let dt = temperature - parameters.temperature;
    let dh = humidity - parameters.humidity;
    let de = erosion - parameters.erosion;

    dt * dt + dh * dh + de * de
}

fn resolve(name: &str) -> Kind {
    Kind::from_name(name).unwrap_or_else(|| panic!("biome references unknown kind {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::{WorldGenConfig, terrain::config::BiomeShapeConfig};

    fn table() -> (WorldGenConfig, BiomeTable) {
        let config = WorldGenConfig::load_default();
        let table = BiomeTable::new(&config.biomes, config.caves.overhang_strength);

        (config, table)
    }

    fn weights(table: &BiomeTable, temperature: f32, humidity: f32, erosion: f32) -> Vec<f32> {
        let mut out = vec![0.0; table.len()];
        table.weights_into(temperature, humidity, erosion, &mut out);
        out
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

    #[test]
    fn weights_are_a_normalized_partition() {
        let (config, table) = table();
        let mut out = vec![0.0; table.len()];

        for step in 0..256 {
            let value = step as f32 * 0.37 - 48.0;
            let dominant = table.weights_into(value, -value * 0.5, value * 0.25, &mut out);
            let sum: f32 = out.iter().sum();

            assert!((sum - 1.0).abs() < 1e-4, "weights sum to {sum}");
            assert!(out.iter().all(|weight| weight.is_finite()));
            assert_eq!(dominant, table.select(value, -value * 0.5, value * 0.25));
            assert!(
                out[dominant] >= out.iter().copied().fold(0.0, f32::max) - f32::EPSILON,
                "dominant entry does not carry the largest weight"
            );
            assert!(dominant < config.biomes.entries.len());
        }
    }

    #[test]
    fn weights_are_continuous_across_a_pairwise_boundary() {
        let (_config, table) = table();
        let midpoint = weights(&table, 0.0, 0.2, 0.2);

        assert!((midpoint[0] - midpoint[1]).abs() < 1e-3, "{midpoint:?}");
        assert!(midpoint[0] > 0.4, "{midpoint:?}");
        assert!(midpoint[1] > 0.4, "{midpoint:?}");

        let before = weights(&table, 0.0, 0.2 - 1e-3, 0.2);
        let after = weights(&table, 0.0, 0.2 + 1e-3, 0.2);

        for index in 0..table.len() {
            assert!(
                (before[index] - after[index]).abs() < 5e-2,
                "weight {index} jumped across the boundary: {} -> {}",
                before[index],
                after[index]
            );
        }
    }

    #[test]
    fn shape_parameters_do_not_affect_selection() {
        let (mut config, reference) = table();

        for (index, biome) in config.biomes.entries.iter_mut().enumerate() {
            let sign = if index.is_multiple_of(2) { 1.0 } else { -1.0 };
            biome.shape.height_offset = 10.0 * sign;
            biome.shape.relief_multiplier = 1.5;
            biome.shape.terrace_strength = 0.5 * sign.abs();
            biome.shape.cave_density = 0.5;
            biome.shape.overhang_strength = 3.0;
        }

        let changed = BiomeTable::new(&config.biomes, config.caves.overhang_strength);
        let mut reference_weights = vec![0.0; reference.len()];
        let mut changed_weights = vec![0.0; changed.len()];

        for step in 0..256 {
            let value = step as f32 * 0.31 - 40.0;
            let reference_dominant =
                reference.weights_into(value, -value, value * 0.5, &mut reference_weights);
            let changed_dominant =
                changed.weights_into(value, -value, value * 0.5, &mut changed_weights);

            assert_eq!(reference_dominant, changed_dominant);
            assert_eq!(reference_weights, changed_weights);
        }
    }

    #[test]
    fn non_uniform_shape_values_blend_by_weight() {
        let mut config = WorldGenConfig::load_default();
        config.biomes.entries.truncate(2);
        config.caves.overhang_strength = 6.0;
        config.biomes.entries[0].shape = BiomeShapeConfig {
            height_offset: 20.0,
            relief_multiplier: 1.6,
            terrace_strength: 0.4,
            cave_density: 0.2,
            overhang_strength: 2.0,
        };
        config.biomes.entries[1].shape = BiomeShapeConfig {
            height_offset: -8.0,
            relief_multiplier: 0.4,
            terrace_strength: 0.1,
            cave_density: 1.8,
            overhang_strength: 5.0,
        };
        let table = BiomeTable::new(&config.biomes, config.caves.overhang_strength);
        let shape = table.blended_shape(&[0.25, 0.75]);

        assert!((shape.height_offset - (-1.0)).abs() < 1e-5);
        assert!((shape.relief_multiplier - 0.7).abs() < 1e-5);
        assert!((shape.terrace_strength - 0.175).abs() < 1e-5);
        assert!((shape.cave_density - 1.4).abs() < 1e-5);
        assert!((shape.overhang_strength - 4.25).abs() < 1e-5);
    }

    #[test]
    fn offset_and_relief_blend_at_an_equidistant_point() {
        let mut config = WorldGenConfig::load_default();
        config.biomes.entries.truncate(2);
        config.biomes.entries[0].shape.height_offset = 20.0;
        config.biomes.entries[0].shape.relief_multiplier = 1.4;
        config.biomes.entries[1].shape.height_offset = 4.0;
        config.biomes.entries[1].shape.relief_multiplier = 1.0;

        let table = BiomeTable::new(&config.biomes, config.caves.overhang_strength);
        let midpoint = weights(&table, 0.0, 0.2, 0.2);

        assert!((midpoint[0] - 0.5).abs() < 1e-3, "{midpoint:?}");
        assert!((midpoint[1] - 0.5).abs() < 1e-3, "{midpoint:?}");

        let shape = table.blended_shape(&midpoint);
        assert!((shape.height_offset - 12.0).abs() < 1e-3);
        assert!((shape.relief_multiplier - 1.2).abs() < 1e-3);
    }

    #[test]
    fn surface_biome_is_weighted_and_deterministic() {
        let table_config = WorldGenConfig::load_default();
        let table = BiomeTable::new(&table_config.biomes, table_config.caves.overhang_strength);

        // One-hot weights always resolve to that entry.
        for step in 0..16 {
            let jitter = step as f32 / 16.0;
            let mut weights = vec![0.0; table.len()];
            weights[2] = 1.0;
            assert_eq!(table.surface_biome(&weights, jitter), 2);
        }

        // Mixed weights follow the cumulative distribution.
        let mut weights = vec![0.0; table.len()];
        weights[0] = 0.3;
        weights[1] = 0.7;
        assert_eq!(table.surface_biome(&weights, 0.1), 0);
        assert_eq!(table.surface_biome(&weights, 0.5), 1);
        assert_eq!(table.surface_biome(&weights, 0.95), 1);

        for _ in 0..8 {
            assert_eq!(table.surface_biome(&weights, 0.42), 1);
        }
    }

    #[test]
    fn tie_resolves_to_the_first_entry() {
        let (_config, table) = table();
        let midpoint = weights(&table, 0.0, 0.2, 0.2);

        assert!((midpoint[0] - midpoint[1]).abs() < 1e-3, "{midpoint:?}");
        assert_eq!(table.select(0.0, 0.2, 0.2), 0);
    }

    #[test]
    fn neutral_shapes_blend_to_their_neutral_values() {
        let mut config = WorldGenConfig::load_default();
        for entry in &mut config.biomes.entries {
            entry.shape.height_offset = 0.0;
            entry.shape.relief_multiplier = 1.0;
            entry.shape.terrace_strength = 0.0;
            entry.shape.cave_density = 1.0;
            entry.shape.overhang_strength = config.caves.overhang_strength;
        }
        let table = BiomeTable::new(&config.biomes, config.caves.overhang_strength);

        for step in 0..64 {
            let value = step as f32 * 0.13 - 4.0;
            let weights = weights(&table, value, -value, value * 0.5);
            let shape = table.blended_shape(&weights);

            assert_eq!(shape.height_offset, 0.0);
            assert_eq!(shape.relief_multiplier, 1.0);
            assert_eq!(shape.terrace_strength, 0.0);
            assert_eq!(shape.cave_density, 1.0);
            assert_eq!(shape.overhang_strength, 6.0);
        }
    }
}
