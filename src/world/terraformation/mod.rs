use bevy::prelude::*;

use projekto_core::{landscape, voxel};

mod genesis;
mod landscaping;

pub mod prelude {
    pub use super::genesis::events;
    pub use super::genesis::ChunkKindRes;
    pub use super::genesis::ChunkLightRes;
    pub use super::genesis::ChunkVertexRes;
    pub use super::genesis::GenesisCommandBuffer;
    // pub use super::terraforming::ChunkSystemQuery;
    // pub use super::terraforming::ChunkSystemRaycast;
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

type VoxelUpdateList = Vec<(IVec3, voxel::Kind)>;
