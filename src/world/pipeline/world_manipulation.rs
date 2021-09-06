use bevy::prelude::*;

use crate::world::storage::{voxel, World};

pub(super) struct WorldManipulationPlugin;

impl Plugin for WorldManipulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CmdChunkAdd>()
            .add_event::<CmdChunkRemove>()
            .add_event::<CmdChunkUpdate>()
            .add_event::<EvtChunkAdded>()
            .add_event::<EvtChunkUpdated>()
            .add_event::<EvtChunkRemoved>()
            .add_startup_system_to_stage(super::PipelineStartup::WorldManipulation, setup_world)
            .add_system_set_to_stage(
                super::Pipeline::WorldManipulation,
                SystemSet::new()
                    .with_system(process_add_chunks_system.label("add"))
                    .with_system(process_remove_chunks_system.label("remove").after("add"))
                    .with_system(process_update_chunks_system.after("remove")),
            );
    }
}

pub struct CmdChunkAdd(pub IVec3);
pub struct CmdChunkRemove(pub IVec3);
pub struct CmdChunkUpdate(pub IVec3, pub IVec3, pub voxel::Kind);

pub struct EvtChunkAdded(pub IVec3);
pub struct EvtChunkRemoved(pub IVec3);
pub struct EvtChunkUpdated(pub IVec3, pub IVec3);

fn setup_world(mut commands: Commands) {
    commands.insert_resource(World::default());
}

fn process_add_chunks_system(
    mut world: ResMut<World>,
    mut reader: EventReader<CmdChunkAdd>,
    mut writer: EventWriter<EvtChunkAdded>,
) {
    for CmdChunkAdd(local) in reader.iter() {
        world.add(*local);
        writer.send(EvtChunkAdded(*local));
    }
}

fn process_remove_chunks_system(
    mut world: ResMut<World>,
    mut reader: EventReader<CmdChunkRemove>,
    mut writer: EventWriter<EvtChunkRemoved>,
) {
    for CmdChunkRemove(local) in reader.iter() {
        world.remove(*local);
        writer.send(EvtChunkRemoved(*local));
    }
}

fn process_update_chunks_system(
    mut world: ResMut<World>,
    mut reader: EventReader<CmdChunkUpdate>,
    mut writer: EventWriter<EvtChunkUpdated>,
) {
    for CmdChunkUpdate(chunk_local, voxel_local, voxel_value) in reader.iter() {
        if !world.exists(*chunk_local) {
            warn!(
                "Skipping update on {} {} since the chunk doesn't exists",
                *chunk_local, voxel_local
            );
            continue;
        }

        world[*chunk_local].set_voxel_kind(*voxel_local, *voxel_value);
        writer.send(EvtChunkUpdated(*chunk_local, *voxel_local));
    }
}
