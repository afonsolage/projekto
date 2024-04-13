use bevy::{
    asset::{Asset, AssetLoader},
    reflect::{
        serde::TypedReflectDeserializer, FromReflect, GetTypeRegistration, Reflect, TypeRegistry,
    },
    utils::HashMap,
};
use futures_lite::AsyncReadExt;
use noise::{
    core::worley::{self, distance_functions, ReturnType},
    permutationtable::PermutationTable,
    Add, Billow, Blend, Clamp, Constant, Curve, Exponent, Fbm, Max, Min, MultiFractal, Multiply,
    NoiseFn, Perlin, RidgedMulti, ScaleBias, Seedable, Select, Terrace, Turbulence, Worley,
};
use serde::de::DeserializeSeed;

pub type BoxedNoiseFn = Box<dyn NoiseFn<f64, 3> + Send>;

#[derive(Debug, Clone, Default, Reflect)]
pub enum WorleySpecReturnType {
    #[default]
    Value,
    Distance,
}

impl From<WorleySpecReturnType> for worley::ReturnType {
    fn from(value: WorleySpecReturnType) -> Self {
        match value {
            WorleySpecReturnType::Value => worley::ReturnType::Value,
            WorleySpecReturnType::Distance => worley::ReturnType::Distance,
        }
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
    Billow {
        seed: u32,
        frequency: f64,
        octaves: usize,
        lacunarity: f64,
        persistence: f64,
    },
    Worley {
        seed: u32,
        frequency: f64,
        return_type: WorleySpecReturnType,
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
    Max {
        source_1: String,
        source_2: String,
    },
    Multiply {
        source_1: String,
        source_2: String,
    },
    Add {
        source_1: String,
        source_2: String,
    },
    Clamp {
        source: String,
        bounds: (f64, f64),
    },
    Turbulence {
        source: String,
        seed: u32,
        frequency: f64,
        power: f64,
        roughness: usize,
    },
    Select {
        source_1: String,
        source_2: String,
        control: String,
        bounds: (f64, f64),
        falloff: f64,
    },
    Terrace {
        source: String,
        control_points: Vec<f64>,
    },
    RidgedMulti {
        seed: u32,
        frequency: f64,
        lacunarity: f64,
        octaves: usize,
    },
    Constant(f64),
    Blend {
        source_1: String,
        source_2: String,
        control: String,
    },
    Exponent {
        source: String,
        exponent: f64,
    },
}

impl NoiseFnSpec {
    pub fn dependencies(&self) -> Vec<&str> {
        match self {
            // No Sources
            NoiseFnSpec::Fbm { .. }
            | NoiseFnSpec::RidgedMulti { .. }
            | NoiseFnSpec::Worley { .. }
            | NoiseFnSpec::Billow { .. }
            | NoiseFnSpec::Constant(..) => vec![],
            // Single Sources
            NoiseFnSpec::Curve { source, .. }
            | NoiseFnSpec::ScaleBias { source, .. }
            | NoiseFnSpec::Turbulence { source, .. }
            | NoiseFnSpec::Terrace { source, .. }
            | NoiseFnSpec::Exponent { source, .. }
            | NoiseFnSpec::Clamp { source, .. } => {
                vec![source]
            }
            // Two sources
            NoiseFnSpec::Min { source_1, source_2 }
            | NoiseFnSpec::Max { source_1, source_2 }
            | NoiseFnSpec::Add { source_1, source_2 }
            | NoiseFnSpec::Multiply { source_1, source_2 } => {
                vec![source_1, source_2]
            }
            // Three sources
            NoiseFnSpec::Select {
                source_1,
                source_2,
                control: source_3,
                ..
            }
            | NoiseFnSpec::Blend {
                source_1,
                source_2,
                control: source_3,
            } => vec![source_1, source_2, source_3],
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NoiseStackError {
    #[error("Failed to load noise stack: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to load noise stack: {0}")]
    RonDeserialize(#[from] ron::error::SpannedError),
    #[error("Failed to load noise stack: {0}")]
    Deserialize(#[from] ron::error::Error),
    #[error("Failed to load noise stack: Reflect error")]
    Reflect,
    #[error("Failed to load noise stack: No spec was found")]
    SpecEmpty,
    #[error("Failed to load noise stack: No main spec was found")]
    SpecNoMain,
    #[error("Failed to load noise stack: Missing dependencies on spec.")]
    MissingDepSpec,
    #[error("Multiple keys detected: {0}")]
    MultipleSpecKeys(String),
}

#[derive(Default, Reflect, Clone)]
struct RawNoiseStack {
    specs: Vec<(String, NoiseFnSpec)>,
}

impl RawNoiseStack {
    fn parse_tree(self) -> Result<NoiseStack, NoiseStackError> {
        if self.specs.is_empty() {
            return Err(NoiseStackError::SpecEmpty);
        }

        if !self.specs.iter().any(|(name, _)| name == "main") {
            return Err(NoiseStackError::SpecNoMain);
        }

        if let Some((name, _)) = self
            .specs
            .iter()
            .fold(HashMap::new(), |mut map, (name, _)| {
                *map.entry(name).or_insert(0usize) += 1;

                if *map.get(name).unwrap() > 1 {
                    bevy::log::warn!("Duplicated spec name: {name}");
                }

                map
            })
            .into_iter()
            .find(|(_, v)| *v > 1)
        {
            return Err(NoiseStackError::MultipleSpecKeys(name.to_string()));
        }

        let mut invalid = false;
        for (name, spec) in &self.specs {
            for dep in spec.dependencies() {
                if !self.specs.iter().any(|(name, _)| name == dep) {
                    bevy::log::warn!("Dependency {dep} not found on spec {name}");
                    invalid = true;
                }
            }
        }

        if invalid {
            Err(NoiseStackError::MissingDepSpec)
        } else {
            bevy::log::debug!("Noise tree loaded.");
            Ok(NoiseStack {
                specs: self.specs.into_iter().collect(),
            })
        }
    }
}

#[derive(Clone)]
pub struct StaticWorley {
    pub frequency: f64,
    pub return_type: ReturnType,
    pub distance_fn: fn(&[f64], &[f64]) -> f64,
    seed: u32,
    perm_table: PermutationTable,
}

impl StaticWorley {
    pub fn new(seed: u32) -> Self {
        StaticWorley {
            frequency: 1.0,
            return_type: ReturnType::Value,
            distance_fn: distance_functions::euclidean,
            seed,
            perm_table: PermutationTable::new(seed),
        }
    }
}

impl NoiseFn<f64, 3> for StaticWorley {
    fn get(&self, point: [f64; 3]) -> f64 {
        todo!()
    }
}

#[derive(Asset, Debug, Default, Reflect, Clone)]
pub struct NoiseStack {
    specs: HashMap<String, NoiseFnSpec>,
}

impl NoiseStack {
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, NoiseStackError> {
        let content = std::fs::read(path)?;
        Self::from_bytes(&content)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, NoiseStackError> {
        let mut registry = TypeRegistry::new();
        registry.register::<(f64, f64)>();
        registry.register::<Vec<(f64, f64)>>();
        registry.register::<Vec<f64>>();
        registry.register::<NoiseFnSpec>();
        registry.register::<RawNoiseStack>();
        registry.register::<(String, NoiseFnSpec)>();
        registry.register::<Vec<(String, NoiseFnSpec)>>();

        let registration = <RawNoiseStack as GetTypeRegistration>::get_type_registration();
        let mut deserializer = ron::de::Deserializer::from_bytes(bytes)?;
        let reflect_deserializer = TypedReflectDeserializer::new(&registration, &registry);
        let deserialized = reflect_deserializer.deserialize(&mut deserializer)?;

        let Some(raw_stack) = <RawNoiseStack as FromReflect>::from_reflect(&*deserialized) else {
            return Err(NoiseStackError::Reflect);
        };

        raw_stack.parse_tree()
    }

    pub fn new(specs: HashMap<String, NoiseFnSpec>) -> Self {
        Self { specs }
    }

    pub fn build_dep_tree<'a, 'b: 'a>(&'a self, name: &'b str) -> Vec<&'a str> {
        let spec = self.specs.get(name).unwrap();
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
        self.specs.get(name)
    }

    pub fn build(&self, name: &str) -> BoxedNoiseFn {
        let spec = self.specs.get(name).unwrap();

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
            NoiseFnSpec::Worley {
                seed,
                frequency,
                return_type,
            } => {
                // let worley = Worley::new(*seed)
                //     .set_frequency(*frequency)
                //     .set_return_type(*return_type.into());
                let worley = StaticWorley::new(*seed);
                Box::new(worley)
            }
            NoiseFnSpec::Billow {
                seed,
                frequency,
                octaves,
                lacunarity,
                persistence,
            } => {
                let billow = Billow::<Perlin>::new(*seed)
                    .set_frequency(*frequency)
                    .set_octaves(*octaves)
                    .set_lacunarity(*lacunarity)
                    .set_persistence(*persistence);
                Box::new(billow)
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
            NoiseFnSpec::Max { source_1, source_2 } => {
                let source_1 = self.build(source_1);
                let source_2 = self.build(source_2);
                Box::new(Max::new(source_1, source_2))
            }
            NoiseFnSpec::Multiply { source_1, source_2 } => {
                let source_1 = self.build(source_1);
                let source_2 = self.build(source_2);
                Box::new(Multiply::new(source_1, source_2))
            }
            NoiseFnSpec::Add { source_1, source_2 } => {
                let source_1 = self.build(source_1);
                let source_2 = self.build(source_2);
                Box::new(Add::new(source_1, source_2))
            }
            NoiseFnSpec::Clamp { source, bounds } => {
                let source = self.build(source);
                Box::new(Clamp::new(source).set_bounds(bounds.0, bounds.1))
            }
            NoiseFnSpec::Exponent { source, exponent } => {
                let source = self.build(source);
                Box::new(Exponent::new(source).set_exponent(*exponent))
            }
            NoiseFnSpec::Turbulence {
                source,
                seed,
                frequency,
                power,
                roughness,
            } => {
                let source = self.build(source);
                let turbulence = Turbulence::<_, Perlin>::new(source)
                    .set_seed(*seed)
                    .set_frequency(*frequency)
                    .set_power(*power)
                    .set_roughness(*roughness);
                Box::new(turbulence)
            }
            NoiseFnSpec::Select {
                source_1,
                source_2,
                control,
                bounds,
                falloff,
            } => {
                let source_1 = self.build(source_1);
                let source_2 = self.build(source_2);
                let control = self.build(control);
                let select = Select::new(source_1, source_2, control)
                    .set_bounds(bounds.0, bounds.1)
                    .set_falloff(*falloff);
                Box::new(select)
            }
            NoiseFnSpec::Terrace {
                source,
                control_points: control_ponts,
            } => {
                let source = self.build(source);
                let terrace = control_ponts
                    .iter()
                    .copied()
                    .fold(Terrace::new(source), |t, p| t.add_control_point(p));

                Box::new(terrace)
            }
            NoiseFnSpec::RidgedMulti {
                seed,
                frequency,
                lacunarity,
                octaves,
            } => {
                let ridged_multi = RidgedMulti::<Perlin>::new(*seed)
                    .set_frequency(*frequency)
                    .set_lacunarity(*lacunarity)
                    .set_octaves(*octaves);
                Box::new(ridged_multi)
            }
            NoiseFnSpec::Constant(value) => Box::new(Constant::new(*value)),
            NoiseFnSpec::Blend {
                source_1,
                source_2,
                control,
            } => {
                let source_1 = self.build(source_1);
                let source_2 = self.build(source_2);
                let control = self.build(control);
                let blend = Blend::new(source_1, source_2, control);
                Box::new(blend)
            }
        }
    }

    pub fn main(&self) -> BoxedNoiseFn {
        self.build("main")
    }
}

#[derive(Default)]
pub struct NoiseStackLoader;

impl AssetLoader for NoiseStackLoader {
    type Asset = NoiseStack;

    type Settings = ();

    type Error = NoiseStackError;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        _load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            NoiseStack::from_bytes(&bytes)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load() {
        let path = format!("{}/noises/world_surface.ron", env!("ASSETS_PATH"));
        NoiseStack::load(path).unwrap();
    }
}
