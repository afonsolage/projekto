use bevy::{reflect::Reflect, utils::HashMap};
use noise::{Clamp, Curve, Fbm, Min, MultiFractal, NoiseFn, Perlin, ScaleBias};

pub type BoxedNoiseFn = Box<dyn NoiseFn<f64, 3> + Send>;

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
}

#[derive(Debug, Default, Reflect, Clone)]
pub struct NoiseStack {
    spec_map: HashMap<String, NoiseFnSpec>,
}

impl NoiseStack {
    pub fn new(spec_map: HashMap<String, NoiseFnSpec>) -> Self {
        Self { spec_map }
    }

    pub fn build_dep_tree<'a, 'b: 'a>(&'a self, name: &'b str) -> Vec<&'a str> {
        let spec = self.spec_map.get(name).unwrap();
        let dependencies = spec.dependencies();

        std::iter::once(name)
            .chain(
                dependencies
                    .into_iter()
                    .flat_map(|name| self.build_dep_tree(name)),
            )
            .collect::<Vec<&str>>()
    }

    pub fn get_spec(&self, name: &str) -> Option<&NoiseFnSpec> {
        self.spec_map.get(name)
    }

    pub fn build(&self, name: &str) -> BoxedNoiseFn {
        let spec = self.spec_map.get(name).unwrap();

        match spec {
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
                let source = self.build(source);
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
                let source = self.build(source);
                Box::new(ScaleBias::new(source).set_scale(*scale).set_bias(*bias))
            }
            NoiseFnSpec::Min { source_1, source_2 } => {
                let source_1 = self.build(source_1);
                let source_2 = self.build(source_2);
                Box::new(Min::new(source_1, source_2))
            }
            NoiseFnSpec::Clamp { source, bounds } => {
                let source = self.build(source);
                Box::new(Clamp::new(source).set_bounds(bounds.0, bounds.1))
            }
        }
    }

    pub fn main(&self) -> BoxedNoiseFn {
        self.build("main")
    }
}
