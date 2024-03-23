use bevy::{reflect::Reflect, utils::HashMap};
use noise::{Cache, Clamp, Curve, Fbm, Min, MultiFractal, NoiseFn, Perlin, ScaleBias};

type BoxedNoiseFn = Box<dyn NoiseFn<f64, 3>>;

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

pub struct NoiseStack {
    seed: u32,
    map: HashMap<String, NoiseFnSpec>,
    noise_name: String,
    noise_fn: BoxedNoiseFn,
}

impl NoiseStack {
    fn new(name: &str, seed: u32, spec_map: HashMap<String, NoiseFnSpec>) -> Self {
        let noise_fn = spec_map.get(name).unwrap().instanciate(&spec_map);
        Self {
            seed,
            map: spec_map,
            noise_name: name.to_string(),
            noise_fn,
        }
    }

    fn build_dep_tree<'a, 'b: 'a>(&'a self, name: &'b str) -> Vec<&'a str> {
        let spec = self.map.get(name).unwrap();
        let dependencies = spec.dependencies();

        std::iter::once(name)
            .chain(
                dependencies
                    .into_iter()
                    .flat_map(|name| self.build_dep_tree(name)),
            )
            .collect::<Vec<&str>>()
    }

    pub fn get(&self, x: f64, z: f64) -> f64 {
        self.noise_fn.get([x, 0.0, z])
    }
}

pub fn create_terrain_stack() -> NoiseStack {
    let mut map = HashMap::new();

    let continent = NoiseFnSpec::Fbm {
        seed: 42,
        frequency: 1.0,
        octaves: 14,
        lacunarity: 2.2089,
        persistence: 0.5,
    };
    map.insert("continent".to_string(), continent);

    let curve = NoiseFnSpec::Curve {
        source: "continent".to_string(),
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
    map.insert("curve".to_string(), curve);

    let carver = NoiseFnSpec::Fbm {
        seed: 42,
        frequency: 4.3437,
        octaves: 11,
        lacunarity: 2.2089,
        persistence: 0.5,
    };
    map.insert("carver".to_string(), carver);

    let scaled_carver = NoiseFnSpec::ScaleBias {
        source: "carver".to_string(),
        scale: 0.375,
        bias: 0.625,
    };
    map.insert("scaled_carver".to_string(), scaled_carver);

    let carved_continent = NoiseFnSpec::Min {
        source_1: "scaled_carver".to_string(),
        source_2: "curve".to_string(),
    };
    map.insert("carved_continent".to_string(), carved_continent);

    let clamp = NoiseFnSpec::Clamp {
        source: "carved_continent".to_string(),
        bounds: (-1.0, 1.0),
    };
    map.insert("clamp".to_string(), clamp);

    let stack = NoiseStack::new("clamp", 42, map);
    let dependency_tree = stack.build_dep_tree("clamp");
    dbg!(dependency_tree);
    stack
}
