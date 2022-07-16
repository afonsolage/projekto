use bevy::prelude::*;

use super::storage::landscape;

mod genesis;
mod landscaping;

pub mod prelude {
    pub use super::genesis::BatchChunkCmdRes;
    pub use super::genesis::EvtChunkUpdated;
    pub use super::genesis::WorldRes;
}

pub struct TerraformationPlugin;

impl Plugin for TerraformationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(genesis::GenesisPlugin)
            .add_plugin(landscaping::LandscapingPlugin)
            .insert_resource(TerraformationConfig {
                horizontal_radius: (landscape::HORIZONTAL_RADIUS + 2) as u32,
            });
    }
}

#[derive(Component)]
pub struct TerraformationCenter;

#[derive(Default)]
pub struct TerraformationConfig {
    pub horizontal_radius: u32,
}
