use bevy::pbr::MaterialPipelineKey;
use bevy::prelude::*;

use bevy::render::render_resource::{AsBindGroup, ShaderRef, VertexFormat};
use bevy::{pbr::MaterialPipeline, reflect::TypeUuid, render::mesh::MeshVertexAttribute};

#[derive(Component)]
pub struct ChunkMaterialHandle(pub Handle<ChunkMaterial>);

#[derive(AsBindGroup, Debug, Clone, TypeUuid)]
#[uuid = "f690fd1e-d5d8-45ab-8225-97e2a3f056e0"]
pub struct ChunkMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    #[uniform(2)]
    pub tile_texture_size: f32,
}

impl ChunkMaterial {
    pub const ATTRIBUTE_TILE_COORD_START: MeshVertexAttribute =
        MeshVertexAttribute::new("TileCoordStart", 66438, VertexFormat::Float32x2);

    pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
        MeshVertexAttribute::new("Light", 66439, VertexFormat::Float32x3);
}
impl Material for ChunkMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/voxel.wgsl".into()
    }

    fn vertex_shader() -> ShaderRef {
        "shaders/voxel.wgsl".into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::render::mesh::MeshVertexBufferLayout,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        let vertex_layout = layout.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            ChunkMaterial::ATTRIBUTE_TILE_COORD_START.at_shader_location(3),
            ChunkMaterial::ATTRIBUTE_LIGHT.at_shader_location(4),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}
