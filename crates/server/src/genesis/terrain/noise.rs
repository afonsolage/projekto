//! Named fractal noise fields sampled in world coordinates.

use bracket_noise::prelude::{FastNoise, FractalType, NoiseType};

use super::config::FieldConfig;

/// A fractal simplex noise field configured from a [`FieldConfig`].
pub(crate) struct NoiseField {
    noise: FastNoise,
}

impl NoiseField {
    pub(crate) fn new(config: &FieldConfig, seed: u64) -> Self {
        let mut noise = FastNoise::seeded(seed);
        noise.set_noise_type(NoiseType::SimplexFractal);
        noise.set_fractal_type(FractalType::FBM);
        noise.set_frequency(config.frequency);
        noise.set_fractal_octaves(config.octaves as i32);
        noise.set_fractal_gain(config.gain);
        noise.set_fractal_lacunarity(config.lacunarity);

        Self { noise }
    }

    pub(crate) fn sample(&self, x: f32, z: f32) -> f32 {
        self.noise.get_noise(x, z)
    }

    pub(crate) fn sample_3d(&self, x: f32, y: f32, z: f32) -> f32 {
        self.noise.get_noise3d(x, y, z)
    }
}

/// Derives an independent seed for a named field.
pub(crate) fn derive_seed(seed: u64, tag: u64) -> u64 {
    splitmix64(seed ^ tag.wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> FieldConfig {
        FieldConfig {
            frequency: 0.01,
            octaves: 3,
            gain: 0.5,
            lacunarity: 2.0,
        }
    }

    #[test]
    fn same_seed_samples_identically() {
        let a = NoiseField::new(&config(), derive_seed(42, 1));
        let b = NoiseField::new(&config(), derive_seed(42, 1));

        for step in 0..64 {
            let x = step as f32 * 3.7;
            assert_eq!(a.sample(x, -x), b.sample(x, -x));
            assert_eq!(a.sample_3d(x, 12.0, -x), b.sample_3d(x, 12.0, -x));
        }
    }

    #[test]
    fn different_seeds_differ() {
        let a = NoiseField::new(&config(), derive_seed(42, 1));
        let b = NoiseField::new(&config(), derive_seed(43, 1));

        let differs = (0..64).any(|step| {
            let x = step as f32 * 3.7;
            a.sample(x, 0.0) != b.sample(x, 0.0)
        });

        assert!(differs);
    }

    #[test]
    fn field_layers_get_different_seeds() {
        assert_ne!(derive_seed(42, 1), derive_seed(42, 2));
        assert_ne!(derive_seed(42, 1), derive_seed(43, 1));
    }

    #[test]
    fn samples_are_finite() {
        let field = NoiseField::new(&config(), derive_seed(7, 3));

        for step in 0..128 {
            let x = step as f32 * 1.3 - 40.0;
            let value = field.sample(x, -x);
            assert!(value.is_finite());

            let value = field.sample_3d(x, 100.0, -x);
            assert!(value.is_finite());
        }
    }
}
