use bevy::prelude::*;

use super::storage::{landscape, chunk::ChunkStorage, voxel};

mod genesis;
mod landscaping;
mod terraforming;

pub mod prelude {
    pub use super::genesis::BatchChunkCmdRes;
    pub use super::genesis::EvtChunkUpdated;
    pub use super::genesis::WorldRes;
    pub use super::terraforming::CmdChunkUpdate;
    pub use super::terraforming::ChunkSystemQuery;
    pub use super::terraforming::ChunkSystemRaycast;
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

pub type ChunkFacesOcclusion = ChunkStorage<voxel::FacesOcclusion>;

impl ChunkFacesOcclusion {
    pub fn is_fully_occluded(&self) -> bool {
        self.iter().all(voxel::FacesOcclusion::is_fully_occluded)
    }
}