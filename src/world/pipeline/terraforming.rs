use bevy::prelude::*;

use crate::world::storage::voxel;

use super::genesis::BatchChunkCmdRes;

pub(super) struct TerraformingPlugin;

impl Plugin for TerraformingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CmdChunkUpdate>()
            .add_event::<EvtChunkUpdatedOld>()
            .add_system_set_to_stage(
                super::Pipeline::Terraforming,
                SystemSet::new().with_system(process_update_chunks_system),
            );
    }
}

#[derive(Clone)]
pub struct CmdChunkUpdate(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

#[derive(Clone, Copy)]
pub struct EvtChunkUpdatedOld(pub IVec3);

fn process_update_chunks_system(
    mut reader: EventReader<CmdChunkUpdate>,
    mut batch: ResMut<BatchChunkCmdRes>,
) {
    let mut _perf = perf_fn!();

    for CmdChunkUpdate(local, voxels) in reader.iter() {
        batch.update(*local, voxels.clone());
    }
}
