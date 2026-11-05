//! Surface material rules.

use projekto_core::voxel::Kind;

use super::config::SurfaceConfig;

/// Chooses the kind for a voxel based on its depth below the top of a column.
pub(crate) struct SurfaceRules {
    max_slope: f32,
    top: Kind,
    subsurface: Kind,
    subsurface_depth: i32,
    steep: Kind,
    deep: Kind,
}

impl SurfaceRules {
    pub(crate) fn new(config: &SurfaceConfig) -> Self {
        Self {
            max_slope: config.max_slope,
            top: resolve(&config.top),
            subsurface: resolve(&config.subsurface),
            subsurface_depth: config.subsurface_depth as i32,
            steep: resolve(&config.steep),
            deep: resolve(&config.deep),
        }
    }

    /// Kind used for the bulk of the column.
    pub(crate) fn deep(&self) -> Kind {
        self.deep
    }

    /// Kind at `depth` below the top of a surface run.
    pub(crate) fn kind(&self, depth: i32, slope: f32) -> Kind {
        if slope > self.max_slope {
            return self.steep;
        }

        match depth {
            0 => self.top,
            depth if depth <= self.subsurface_depth => self.subsurface,
            _ => self.deep,
        }
    }
}

fn resolve(name: &str) -> Kind {
    Kind::from_name(name).unwrap_or_else(|| panic!("surface rule references unknown kind {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> SurfaceRules {
        SurfaceRules::new(&SurfaceConfig {
            max_slope: 3.0,
            top: "Grass".to_string(),
            subsurface: "Dirt".to_string(),
            subsurface_depth: 3,
            steep: "Rock".to_string(),
            deep: "Rock".to_string(),
        })
    }

    fn name(kind: Kind) -> &'static str {
        kind.name()
    }

    #[test]
    fn flat_ground_gets_grass_then_dirt() {
        let rules = rules();

        assert_eq!(name(rules.kind(0, 0.0)), "Grass");
        assert_eq!(name(rules.kind(1, 0.0)), "Dirt");
        assert_eq!(name(rules.kind(3, 0.0)), "Dirt");
        assert_eq!(name(rules.kind(4, 0.0)), "Rock");
    }

    #[test]
    fn steep_ground_gets_rock() {
        let rules = rules();

        for depth in 0..6 {
            assert_eq!(name(rules.kind(depth, 5.0)), "Rock");
        }
    }
}
