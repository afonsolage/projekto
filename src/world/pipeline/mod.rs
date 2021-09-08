use bevy::{prelude::*, render::pipeline::PipelineDescriptor, utils::HashMap};

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
        app.add_stage(Pipeline::WorldManipulation, SystemStage::parallel())
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
        app.add_plugin(WorldManipulationPlugin)
            .add_plugin(EntityManagingPlugin)
            .add_plugin(RenderingPlugin);
    }
}

pub struct EvtChunkDirty(pub IVec3);

pub struct ChunkLocal(pub IVec3);

pub struct ChunkEntityMap(pub HashMap<IVec3, Entity>);

pub struct ChunkPipeline(Handle<PipelineDescriptor>);

struct ChunkFacesOcclusion([voxel::FacesOcclusion; chunk::BUFFER_SIZE]);
struct ChunkVertices([Vec<[f32; 3]>; voxel::SIDE_COUNT]);

#[derive(Bundle)]
pub struct ChunkBundle {
    local: ChunkLocal,
    #[bundle]
    mesh_bundle: MeshBundle,
    #[bundle]
    building: ChunkBuildingBundle,
}

impl Default for ChunkBundle {
    fn default() -> Self {
        Self {
            local: ChunkLocal(IVec3::ZERO),
            mesh_bundle: MeshBundle::default(),
            building: ChunkBuildingBundle::default(),
        }
    }
}

#[derive(Bundle)]
pub struct ChunkBuildingBundle {
    faces_occlusion: ChunkFacesOcclusion,
    vertices: ChunkVertices,
}

impl Default for ChunkBuildingBundle {
    fn default() -> Self {
        Self {
            faces_occlusion: ChunkFacesOcclusion(
                [voxel::FacesOcclusion::default(); chunk::BUFFER_SIZE],
            ),
            vertices: ChunkVertices([vec![], vec![], vec![], vec![], vec![], vec![]]),
        }
    }
}
