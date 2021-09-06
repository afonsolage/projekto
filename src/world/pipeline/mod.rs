use bevy::prelude::*;

use self::{entity_managing::EntityManagingPlugin, world_manipulation::WorldManipulationPlugin};

mod entity_managing;
mod world_manipulation;

pub use world_manipulation::{
    CmdChunkAdd, CmdChunkRemove, CmdChunkUpdate, EvtChunkAdded, EvtChunkRemoved, EvtChunkUpdated,
};

#[derive(Debug, StageLabel, PartialEq, Eq, Hash, Clone, Copy)]
enum Pipeline {
    WorldManipulation,
    EntityManaging,
    Rendering,
}

#[derive(Debug, StageLabel, PartialEq, Eq, Hash, Clone, Copy)]
enum PipelineStartup {
    WorldManipulation,
    EntityManaging,
    Rendering,
}

pub struct PipelinePlugin;

impl Plugin for PipelinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(WorldManipulationPlugin)
            .add_plugin(EntityManagingPlugin)
            .add_stage(Pipeline::WorldManipulation, SystemStage::parallel())
            .add_stage_after(
                Pipeline::WorldManipulation,
                Pipeline::EntityManaging,
                SystemStage::parallel(),
            )
            .add_stage_after(
                Pipeline::EntityManaging,
                Pipeline::Rendering,
                SystemStage::parallel(),
            )
            .add_startup_stage_after(
                StartupStage::PreStartup,
                PipelineStartup::WorldManipulation,
                SystemStage::parallel(),
            )
            .add_startup_stage_after(
                PipelineStartup::WorldManipulation,
                PipelineStartup::Rendering,
                SystemStage::parallel(),
            );
    }
}


pub struct ChunkLocal(pub IVec3);

