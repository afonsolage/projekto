use bevy::{
    ecs::system::lifetimeless::SRes,
    pbr::MaterialPipeline,
    prelude::*,
    reflect::TypeUuid,
    render::{
        render_asset::RenderAsset,
        render_resource::{BindGroup, BindGroupDescriptor, BindGroupLayoutDescriptor},
        renderer::RenderDevice,
    },
    utils::HashMap,
};

use self::{
    landscaping::LandscapingPlugin, rendering::RenderingPlugin, terraforming::TerraformingPlugin,
};

use super::terraformation::prelude::*;
use super::storage::{chunk::ChunkStorage, voxel};

pub use rendering::MeshGenerationCounter;
pub use landscaping::LandscapeConfig;
pub use terraforming::{ChunkSystemQuery, ChunkSystemRaycast, CmdChunkUpdate, RaycastResult};

mod landscaping;
mod rendering;
mod terraforming;

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
        app.add_plugin(TerraformingPlugin)
            .add_plugin(LandscapingPlugin)
            .add_plugin(RenderingPlugin);
    }
}

pub struct EvtChunkMeshDirty(pub IVec3);

#[derive(Component)]
pub struct ChunkLocal(pub IVec3);

#[derive(Component)]
pub struct ChunkEntityMap(pub HashMap<IVec3, Entity>);

pub type ChunkFacesOcclusion = ChunkStorage<voxel::FacesOcclusion>;

#[derive(Bundle)]
pub struct ChunkBundle {
    local: ChunkLocal,
    #[bundle]
    mesh_bundle: MaterialMeshBundle<ChunkMaterial>,
}

impl Default for ChunkBundle {
    fn default() -> Self {
        Self {
            local: ChunkLocal(IVec3::ZERO),
            mesh_bundle: MaterialMeshBundle::default(),
        }
    }
}

#[derive(Component)]
pub struct ChunkMaterialHandle(pub Handle<ChunkMaterial>);

#[derive(Debug, Clone, TypeUuid)]
#[uuid = "f690fd1e-d5d8-45ab-8225-97e2a3f056e0"]
pub struct ChunkMaterial;

#[derive(Clone)]
pub struct GpuChunkMaterial {
    bind_group: BindGroup,
}

impl Material for ChunkMaterial {
    fn bind_group(
        material: &<Self as RenderAsset>::PreparedAsset,
    ) -> &bevy::render::render_resource::BindGroup {
        &material.bind_group
    }

    fn bind_group_layout(
        render_device: &RenderDevice,
    ) -> bevy::render::render_resource::BindGroupLayout {
        render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[],
            label: None,
        })
    }

    fn fragment_shader(asset_server: &AssetServer) -> Option<Handle<Shader>> {
        Some(asset_server.load("shaders/voxel.wgsl"))
    }

    fn vertex_shader(asset_server: &AssetServer) -> Option<Handle<Shader>> {
        Some(asset_server.load("shaders/voxel.wgsl"))
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::render::mesh::MeshVertexBufferLayout,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        let vertex_layout = layout.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

impl RenderAsset for ChunkMaterial {
    type ExtractedAsset = Self;
    type PreparedAsset = GpuChunkMaterial;
    type Param = (SRes<RenderDevice>, SRes<MaterialPipeline<Self>>);

    fn extract_asset(&self) -> Self::ExtractedAsset {
        self.clone()
    }

    fn prepare_asset(
        _extracted_asset: Self::ExtractedAsset,
        (render_device, material_pipeline): &mut bevy::ecs::system::SystemParamItem<Self::Param>,
    ) -> Result<
        Self::PreparedAsset,
        bevy::render::render_asset::PrepareAssetError<Self::ExtractedAsset>,
    > {
        let bind_group = render_device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &material_pipeline.material_layout,
            entries: &[],
        });

        Ok(GpuChunkMaterial { bind_group })
    }
}
