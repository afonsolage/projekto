use bevy::{reflect::Reflect, utils::HashMap};
use noise::{Cache, Clamp, Curve, Fbm, Min, MultiFractal, NoiseFn, Perlin, ScaleBias};

type BoxedNoiseFn = Box<dyn NoiseFn<f64, 3>>;

#[repr(transparent)]
struct NoiseFnRef<'a>(&'a BoxedNoiseFn);

impl<'a> NoiseFn<f64, 3> for NoiseFnRef<'a> {
    #[inline]
    fn get(&self, point: [f64; 3]) -> f64 {
        self.0.get(point)
    }
}

#[derive(Debug, Clone, Reflect)]
pub enum NoiseFnSpec {
    Fbm {
        seed: u32,
        frequency: f64,
        octaves: usize,
        lacunarity: f64,
        persistence: f64,
    },
    Curve {
        source: String,
        control_points: Vec<(f64, f64)>,
    },
    ScaleBias {
        source: String,
        scale: f64,
        bias: f64,
    },
    Min {
        source_1: String,
        source_2: String,
    },
    Clamp {
        source: String,
        bounds: (f64, f64),
    },
}

impl NoiseFnSpec {
    fn dependencies(&self) -> Vec<&str> {
        match self {
            // No Sources
            NoiseFnSpec::Fbm { .. } => vec![],
            // Single Sources
            NoiseFnSpec::Curve { source, .. }
            | NoiseFnSpec::ScaleBias { source, .. }
            | NoiseFnSpec::Clamp { source, .. } => {
                vec![source]
            }
            // Two sources
            NoiseFnSpec::Min { source_1, source_2 } => vec![source_1, source_2],
        }
    }

    fn instanciate(&self, map: &HashMap<String, NoiseFnSpec>) -> Box<dyn NoiseFn<f64, 3>> {
        match self {
            NoiseFnSpec::Fbm {
                seed,
                frequency,
                octaves,
                lacunarity,
                persistence,
            } => {
                let fbm = Fbm::<Perlin>::new(*seed)
                    .set_frequency(*frequency)
                    .set_octaves(*octaves)
                    .set_lacunarity(*lacunarity)
                    .set_persistence(*persistence);
                Box::new(fbm)
            }
            NoiseFnSpec::Curve {
                source,
                control_points,
            } => {
                let source = map.get(source).unwrap().instanciate(map);
                let curve = control_points
                    .iter()
                    .copied()
                    .fold(Curve::new(source), |c, (input, output)| {
                        c.add_control_point(input, output)
                    });
                Box::new(curve)
            }
            NoiseFnSpec::ScaleBias {
                source,
                scale,
                bias,
            } => {
                let source = map.get(source).unwrap().instanciate(map);
                Box::new(ScaleBias::new(source).set_scale(*scale).set_bias(*bias))
            }
            NoiseFnSpec::Min { source_1, source_2 } => {
                let source_1 = map.get(source_1).unwrap().instanciate(map);
                let source_2 = map.get(source_2).unwrap().instanciate(map);
                Box::new(Min::new(source_1, source_2))
            }
            NoiseFnSpec::Clamp { source, bounds } => {
                let source = map.get(source).unwrap().instanciate(map);
                Box::new(Clamp::new(source).set_bounds(bounds.0, bounds.1))
            }
        }
    }
}

fn _create_terrain_stack() -> NoiseStack {
    let base = NoiseFnSpec::Fbm {
        seed: 42,
        frequency: 1.0,
        octaves: 14,
        lacunarity: 2.2089,
        persistence: 0.5,
    };

    let curve = NoiseFnSpec::Curve {
        source: "base".to_string(),
        control_points: vec![
            (-2.0000, -1.625),
            (-1.0000, -1.375),
            (0.0000, -0.375),
            (0.0625, 0.125),
            (0.1250, 0.250),
            (0.2500, 1.000),
            (0.5000, 0.250),
            (0.7500, 0.250),
            (1.0000, 0.500),
            (2.0000, 0.500),
        ],
    };

    let mut map = HashMap::new();
    map.insert("base".to_string(), base);
    map.insert("curve".to_string(), curve);

    let curve = map.get("curve").unwrap().instanciate(&map);
    todo!()
}

pub struct NoiseStack {
    seed: u32,
    map: HashMap<String, BoxedNoiseFn>,
    // noise_fn: BoxedNoiseFn,
}

impl NoiseStack {
    pub(crate) fn get_height(&self, _x: f32, _z: f32) -> i32 {
        todo!()
    }
}

impl Default for NoiseStack {
    fn default() -> Self {
        todo!()
    }
}
