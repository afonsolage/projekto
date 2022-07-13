use bevy::prelude::*;

mod genesis;
mod landscaping;

pub mod prelude {
    pub use super::genesis::BatchChunkCmdRes;
    pub use super::genesis::EvtChunkLoaded;
    pub use super::genesis::EvtChunkUnloaded;
    pub use super::genesis::EvtChunkUpdated;
    pub use super::genesis::WorldRes;
}

pub struct TerraformationPlugin;

impl Plugin for TerraformationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(genesis::GenesisPlugin)
            .insert_resource(TerraformationConfig {
                horizontal_radius: 10,
                vertical_radius: 10,
            });
    }
}

#[derive(Component)]
pub struct TerraformationCenter;

#[derive(Default)]
pub struct TerraformationConfig {
    pub horizontal_radius: u32,
    pub vertical_radius: u32,
}
