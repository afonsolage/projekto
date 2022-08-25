use bevy::prelude::*;

use projekto_core::landscape;
use projekto_genesis::GenesisPlugin;

mod landscaping;

pub struct TerraformationPlugin;

impl Plugin for TerraformationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(GenesisPlugin)
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
