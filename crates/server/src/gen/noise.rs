use bracket_noise::prelude::*;

pub(crate) struct Noise {
    continentalness: FastNoise,
}

impl Noise {
    pub fn new() -> Self {
        // TODO: Move this to a config per-biome
        let mut continentalness = FastNoise::seeded(42);
        continentalness.set_noise_type(NoiseType::SimplexFractal);
        continentalness.set_frequency(0.03);
        continentalness.set_fractal_type(FractalType::FBM);
        continentalness.set_fractal_octaves(3);
        continentalness.set_fractal_gain(0.9);
        continentalness.set_fractal_lacunarity(0.5);

        Noise { continentalness }
    }

    pub fn stone(&self, x: f32, z: f32) -> i32 {
        let n = self.continentalness.get_noise(x, z);
        100 + (((n + 1.0) / 2.0) * 50.0) as i32
    }
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}
