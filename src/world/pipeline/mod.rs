use bevy::{prelude::*, render::pipeline::PipelineDescriptor, utils::HashMap};

use self::{
    genesis::GenesisPlugin, landscaping::LandscapingPlugin, rendering::RenderingPlugin,
    terraforming::TerraformingPlugin,
};

mod genesis;
mod landscaping;
mod rendering;
mod terraforming;

pub use genesis::{EvtChunkLoaded, EvtChunkUnloaded, EvtChunkUpdated, WorldRes};
pub use landscaping::LandscapeConfig;
pub use terraforming::{ChunkSystemQuery, ChunkSystemRaycast, CmdChunkUpdate, RaycastResult};

use super::storage::{
    chunk::ChunkStorage,
    voxel::{self, VoxelVertex},
};

#[derive(Debug, StageLabel, PartialEq, Eq, Hash, Clone, Copy)]
enum Pipeline {
    Genesis,
    Terraforming,
    Landscaping,
    Rendering,
}

#[derive(Debug, StageLabel, PartialEq, Eq, Hash, Clone, Copy)]
enum PipelineStartup {
    Genesis,
    Terraforming,
    Landscaping,
    Rendering,
}

pub struct PipelinePlugin;

impl Plugin for PipelinePlugin {
    fn build(&self, app: &mut App) {
        app.add_stage(Pipeline::Genesis, SystemStage::parallel())
            .add_stage_after(
                Pipeline::Genesis,
                Pipeline::Terraforming,
                SystemStage::parallel(),
            )
            .add_stage_after(
                Pipeline::Terraforming,
                Pipeline::Landscaping,
                SystemStage::parallel(),
            )
            .add_stage_after(
                Pipeline::Landscaping,
                Pipeline::Rendering,
                SystemStage::parallel(),
            )
            .add_startup_stage_after(
                StartupStage::Startup,
                PipelineStartup::Genesis,
                SystemStage::parallel(),
            )
            .add_startup_stage_after(
                PipelineStartup::Genesis,
                PipelineStartup::Terraforming,
                SystemStage::parallel(),
            )
            .add_startup_stage_after(
                PipelineStartup::Terraforming,
                PipelineStartup::Landscaping,
                SystemStage::parallel(),
            )
            .add_startup_stage_after(
                PipelineStartup::Landscaping,
                PipelineStartup::Rendering,
                SystemStage::parallel(),
            );
        app.add_plugin(GenesisPlugin)
            .add_plugin(TerraformingPlugin)
            .add_plugin(LandscapingPlugin)
            .add_plugin(RenderingPlugin);
    }
}

pub struct EvtChunkMeshDirty(pub IVec3, pub Vec<VoxelVertex>);

pub struct ChunkLocal(pub IVec3);

pub struct ChunkEntityMap(pub HashMap<IVec3, Entity>);

pub struct ChunkPipeline(Handle<PipelineDescriptor>);

pub type ChunkFacesOcclusion = ChunkStorage<voxel::FacesOcclusion>;

#[derive(Bundle)]
pub struct ChunkBundle {
    local: ChunkLocal,
    #[bundle]
    mesh_bundle: MeshBundle,
}

impl Default for ChunkBundle {
    fn default() -> Self {
        Self {
            local: ChunkLocal(IVec3::ZERO),
            mesh_bundle: MeshBundle::default(),
        }
    }
}
