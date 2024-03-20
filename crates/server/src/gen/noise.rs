use bevy::{math::vec2, prelude::*};
use bracket_noise::prelude::*;

pub struct Noise {
    continentalness: FastNoise,
    curve: Vec<Vec2>,
}

impl Noise {
    pub fn new() -> Self {
        // TODO: Move this to a config per-biome
        let mut continentalness = FastNoise::seeded(42);
        continentalness.set_noise_type(NoiseType::SimplexFractal);
        continentalness.set_frequency(0.03);
        continentalness.set_fractal_type(FractalType::FBM);
        continentalness.set_fractal_octaves(3);
        continentalness.set_fractal_gain(1.0);
        continentalness.set_fractal_lacunarity(1.0);

        let curve = vec![
            vec2(-1.0, 50.0),
            vec2(0.3, 100.0),
            vec2(0.4, 150.0),
            vec2(1.0, 150.0),
        ];

        Noise {
            continentalness,
            curve,
        }
    }

    fn lerp(&self, t: f32) -> i32 {
        assert!(self.curve.len() >= 2);

        let min = self.curve.first().unwrap();
        let max = self.curve.last().unwrap();

        assert!(t >= min.x);
        assert!(t <= max.x);

        for segment in self.curve.windows(2) {
            let begin = segment[0];
            let end = segment[1];

            if t >= begin.x && t <= end.x {
                // Normalize 't' within the segment
                let normalized_t = (t - begin.x) / (end.x - begin.x);

                // Linear interpolation
                return (begin + (end - begin) * normalized_t).y as i32;
            }
        }

        unreachable!()
    }

    pub fn stone(&self, x: f32, z: f32) -> i32 {
        let n = self.continentalness.get_noise(x, z);
        self.lerp(n)
    }
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}
