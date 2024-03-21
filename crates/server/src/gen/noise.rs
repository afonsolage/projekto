use bevy::{prelude::*, utils::HashMap};
use bracket_noise::prelude::{FastNoise, FractalType};

#[derive(Debug, Default, Reflect, PartialEq, Eq, Hash)]
pub enum NoiseType {
    #[default]
    OceanLand,
    LandFlatness,
    Instability,
}

#[derive(Deref, DerefMut)]
#[repr(transparent)]
struct NoiseFn(FastNoise);

impl Default for NoiseFn {
    fn default() -> Self {
        Self(FastNoise::new())
    }
}

#[derive(Default, Reflect)]
pub struct NoiseLayer {
    #[reflect(ignore)]
    noise: NoiseFn,
    curve: Vec<Vec2>,
}

impl std::fmt::Debug for NoiseLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoiseLayer")
            .field("noise", &self.noise.get_noise_type())
            .field("curve", &self.curve)
            .finish()
    }
}

impl NoiseLayer {
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

    pub fn noise(&self, x: f32, z: f32) -> f32 {
        self.noise.get_noise(x, z)
    }

    pub fn eval(&self, x: f32, z: f32) -> i32 {
        let n = self.noise(x, z);
        self.lerp(n)
    }
}

#[derive(Debug, Reflect)]
pub struct Noise {
    seed: u64,
    layers: HashMap<NoiseType, NoiseLayer>,
}

impl Noise {
    fn new(seed: u64) -> Self {
        Self {
            seed,
            layers: vec![
                (NoiseType::OceanLand, NoiseLayer::default()),
                (NoiseType::LandFlatness, NoiseLayer::default()),
                (NoiseType::Instability, NoiseLayer::default()),
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
        }
    }

    pub fn set_layer(
        &mut self,
        noise_type: NoiseType,
        frequency: f32,
        octaves: u32,
        gain: f32,
        lacunarity: f32,
        curve: Vec<Vec2>,
    ) {
        let mut noise = FastNoise::seeded(self.seed);
        noise.set_frequency(frequency);
        noise.set_fractal_octaves(octaves as i32);
        noise.set_fractal_gain(gain);
        noise.set_fractal_lacunarity(lacunarity);
        noise.set_fractal_type(FractalType::FBM);

        self.layers.insert(
            noise_type,
            NoiseLayer {
                noise: NoiseFn(noise),
                curve,
            },
        );
    }

    pub fn get_layer(&self, noise_type: NoiseType) -> &NoiseLayer {
        self.layers
            .get(&noise_type)
            .expect("All layers are added by default.")
    }

    pub fn get_height(&self, x: f32, z: f32) -> i32 {
        self.get_layer(NoiseType::OceanLand).eval(x, z)
    }
}

impl Default for Noise {
    fn default() -> Self {
        Self::new(42)
    }
}
