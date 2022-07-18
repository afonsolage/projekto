use bevy::{
    ecs::system::lifetimeless::SRes,
    pbr::MaterialPipeline,
    prelude::*,
    reflect::TypeUuid,
    render::{
        render_asset::{PrepareAssetError, RenderAsset, RenderAssets},
        render_resource::{
            BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
            BindGroupLayoutEntry, BindingResource, BindingType, SamplerBindingType, ShaderStages,
            TextureSampleType, TextureViewDimension,
        },
        renderer::RenderDevice,
    },
    utils::HashMap,
};

use self::{landscaping::LandscapingPlugin, meshing::MeshingPlugin};

use super::terraformation::prelude::*;

pub use landscaping::LandscapeConfig;

mod landscaping;
mod meshing;

pub struct PipelinePlugin;

impl Plugin for PipelinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(LandscapingPlugin).add_plugin(MeshingPlugin);
    }
}

/**
 This event is raised whenever a chunk mesh needs to be redrawn
*/
pub struct EvtChunkMeshDirty(pub IVec3);

#[derive(Component)]
pub struct ChunkLocal(pub IVec3);

#[derive(Component)]
pub struct ChunkEntityMap(pub HashMap<IVec3, Entity>);

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
pub struct ChunkMaterial {
    texture: Handle<Image>,
}

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
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
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
    type ExtractedAsset = ChunkMaterial;
    type PreparedAsset = GpuChunkMaterial;
    type Param = (
        SRes<RenderDevice>,
        SRes<RenderAssets<Image>>,
        SRes<MaterialPipeline<Self>>,
    );

    fn extract_asset(&self) -> Self::ExtractedAsset {
        self.clone()
    }

    fn prepare_asset(
        extracted_asset: Self::ExtractedAsset,
        (render_device, gpu_images, material_pipeline): &mut bevy::ecs::system::SystemParamItem<
            Self::Param,
        >,
    ) -> Result<
        Self::PreparedAsset,
        bevy::render::render_asset::PrepareAssetError<Self::ExtractedAsset>,
    > {
        let gpu_image = match gpu_images.get(&extracted_asset.texture) {
            Some(gpu_image) => gpu_image,
            None => return Err(PrepareAssetError::RetryNextUpdate(extracted_asset)),
        };

        let bind_group = render_device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &material_pipeline.material_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&gpu_image.texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&gpu_image.sampler),
                },
            ],
        });

        Ok(GpuChunkMaterial { bind_group })
    }
}
