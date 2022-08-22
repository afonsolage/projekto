use bevy::prelude::*;

use projekto_core::{landscape, voxel};

mod genesis;
mod landscaping;

pub mod prelude {
    pub use super::genesis::ChunkKindRes;
    pub use super::genesis::ChunkLightRes;
    pub use super::genesis::ChunkVertexRes;
    pub use super::genesis::EvtChunkUpdated;
    pub use super::genesis::KindsAtlasRes;
    // pub use super::terraforming::ChunkSystemQuery;
    // pub use super::terraforming::ChunkSystemRaycast;
    pub use super::CmdChunkUpdate;
}
pub struct TerraformationPlugin;

impl Plugin for TerraformationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(genesis::GenesisPlugin)
            .add_plugin(landscaping::LandscapingPlugin)
            .add_event::<CmdChunkUpdate>()
            .add_system(process_update_chunks)
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

#[derive(Clone)]
pub struct CmdChunkUpdate(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

fn process_update_chunks(
    mut reader: EventReader<CmdChunkUpdate>,
    mut batch: ResMut<genesis::BatchChunkCmdRes>,
) {
    let mut _perf = perf_fn!();

    for CmdChunkUpdate(local, voxels) in reader.iter() {
        batch.update(*local, voxels.clone());
    }
}

