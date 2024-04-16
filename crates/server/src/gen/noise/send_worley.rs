use noise::{
    core::worley::{self, distance_functions},
    permutationtable::PermutationTable,
    NoiseFn, Seedable,
};

// This wrapper class is needed because noise::Worley isn't Send, since internally
// it uses a Rc to hold distance function reference.
#[derive(Clone)]
pub struct SendWorley {
    pub frequency: f64,
    pub return_type: worley::ReturnType,
    pub distance_fn: fn(&[f64], &[f64]) -> f64,
    seed: u32,
    perm_table: PermutationTable,
}

impl SendWorley {
    pub fn new(seed: u32) -> Self {
        SendWorley {
            frequency: 1.0,
            return_type: worley::ReturnType::Value,
            distance_fn: distance_functions::euclidean,
            seed,
            perm_table: PermutationTable::new(seed),
        }
    }

    pub fn set_frequency(self, frequency: f64) -> Self {
        Self { frequency, ..self }
    }

    pub fn set_return_type(self, return_type: worley::ReturnType) -> Self {
        Self {
            return_type,
            ..self
        }
    }

    // pub fn set_distance_fn(self, distance_fn: fn(&[f64], &[f64]) -> f64) -> Self {
    //     Self {
    //         distance_fn,
    //         ..self
    //     }
    // }
}

impl Seedable for SendWorley {
    fn set_seed(self, seed: u32) -> Self {
        if self.seed == seed {
            self
        } else {
            Self {
                seed,
                perm_table: PermutationTable::new(seed),
                ..self
            }
        }
    }

    fn seed(&self) -> u32 {
        self.seed
    }
}

impl NoiseFn<f64, 3> for SendWorley {
    fn get(&self, point: [f64; 3]) -> f64 {
        worley::worley_3d(
            &self.perm_table,
            self.distance_fn,
            self.return_type,
            noise::Vector3::from(point) * self.frequency,
        )
    }
}
