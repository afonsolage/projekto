//! Surface material rules for one biome.

use projekto_core::voxel::Kind;

use super::config::BiomeSurfaceConfig;

/// Chooses the kind for a voxel based on its depth below the top of a column.
pub(crate) struct SurfaceRule {
    top: Kind,
    subsurface: Kind,
    subsurface_depth: i32,
}

impl SurfaceRule {
    pub(crate) fn new(config: &BiomeSurfaceConfig) -> Self {
        Self {
            top: resolve(&config.top),
            subsurface: resolve(&config.subsurface),
            subsurface_depth: config.subsurface_depth as i32,
        }
    }

    /// Number of voxels below the top that use the subsurface kind.
    pub(crate) fn depth(&self) -> i32 {
        self.subsurface_depth
    }

    /// Kind at `depth` below the top of a surface run.
    pub(crate) fn kind(
        &self,
        depth: i32,
        slope: f32,
        max_slope: f32,
        steep: Kind,
        deep: Kind,
    ) -> Kind {
        if slope > max_slope {
            return steep;
        }

        match depth {
            0 => self.top,
            depth if depth <= self.subsurface_depth => self.subsurface,
            _ => deep,
        }
    }
}

fn resolve(name: &str) -> Kind {
    Kind::from_name(name).unwrap_or_else(|| panic!("surface rule references unknown kind {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(surface: BiomeSurfaceConfig) -> SurfaceRule {
        SurfaceRule::new(&surface)
    }

    fn grass_dirt() -> BiomeSurfaceConfig {
        BiomeSurfaceConfig {
            top: "Grass".to_string(),
            subsurface: "Dirt".to_string(),
            subsurface_depth: 3,
        }
    }

    fn name(kind: Kind) -> &'static str {
        kind.name()
    }

    #[test]
    fn flat_ground_gets_grass_then_dirt() {
        let rules = rules(grass_dirt());
        let rock = Kind::from_name("Rock").unwrap();

        assert_eq!(name(rules.kind(0, 0.0, 3.0, rock, rock)), "Grass");
        assert_eq!(name(rules.kind(1, 0.0, 3.0, rock, rock)), "Dirt");
        assert_eq!(name(rules.kind(3, 0.0, 3.0, rock, rock)), "Dirt");
        assert_eq!(name(rules.kind(4, 0.0, 3.0, rock, rock)), "Rock");
    }

    #[test]
    fn steep_ground_gets_rock() {
        let rules = rules(grass_dirt());
        let rock = Kind::from_name("Rock").unwrap();

        for depth in 0..6 {
            assert_eq!(name(rules.kind(depth, 5.0, 3.0, rock, rock)), "Rock");
        }
    }

    #[test]
    fn tundra_depth_uses_two_subsurface_layers() {
        let rules = rules(BiomeSurfaceConfig {
            top: "Snow".to_string(),
            subsurface: "Dirt".to_string(),
            subsurface_depth: 2,
        });
        let rock = Kind::from_name("Rock").unwrap();

        assert_eq!(name(rules.kind(0, 0.0, 3.0, rock, rock)), "Snow");
        assert_eq!(name(rules.kind(2, 0.0, 3.0, rock, rock)), "Dirt");
        assert_eq!(name(rules.kind(3, 0.0, 3.0, rock, rock)), "Rock");
    }
}
