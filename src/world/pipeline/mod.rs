use bevy::prelude::*;

use self::{
    entity_managing::EntityManagingPlugin, rendering::RenderingPlugin,
    world_manipulation::WorldManipulationPlugin,
};

mod entity_managing;
mod rendering;
mod world_manipulation;

pub use world_manipulation::{
    CmdChunkAdd, CmdChunkRemove, CmdChunkUpdate, EvtChunkAdded, EvtChunkRemoved, EvtChunkUpdated,
};

use super::storage::{chunk, voxel};

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
            .add_plugin(RenderingPlugin)
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
                PipelineStartup::EntityManaging,
                SystemStage::parallel(),
            )
            .add_startup_stage_after(
                PipelineStartup::EntityManaging,
                PipelineStartup::Rendering,
                SystemStage::parallel(),
            );
    }
}

pub struct EvtChunkDirty(pub IVec3);

pub struct ChunkLocal(pub IVec3);

struct ChunkFacesOcclusion([voxel::FacesOcclusion; chunk::BUFFER_SIZE]);

#[derive(Bundle)]
pub struct ChunkBundle {
    local: ChunkLocal,
    #[bundle]
    building: ChunkBuildingBundle,
}

impl Default for ChunkBundle {
    fn default() -> Self {
        Self {
            local: ChunkLocal(IVec3::ZERO),
            building: ChunkBuildingBundle::default(),
        }
    }
}

#[derive(Bundle)]
pub struct ChunkBuildingBundle {
    faces_occlusion: ChunkFacesOcclusion,
}

impl Default for ChunkBuildingBundle {
    fn default() -> Self {
        Self {
            faces_occlusion: ChunkFacesOcclusion(
                [voxel::FacesOcclusion::default(); chunk::BUFFER_SIZE],
            ),
        }
    }
}
