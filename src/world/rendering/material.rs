use bevy::pbr::MaterialPipelineKey;
use bevy::prelude::*;

use bevy::render::render_resource::{AsBindGroup, ShaderRef, VertexFormat};
use bevy::{pbr::MaterialPipeline, reflect::TypeUuid, render::mesh::MeshVertexAttribute};

#[derive(Reflect, Component, Debug)]
pub struct ChunkMaterialHandle(pub Handle<ChunkMaterial>);

#[derive(AsBindGroup, Debug, Clone, TypeUuid)]
#[uuid = "f690fd1e-d5d8-45ab-8225-97e2a3f056e0"]
pub struct ChunkMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    #[uniform(2)]
    pub tile_texture_size: f32,
    #[uniform(2)]
    pub clip_map_origin: Vec2,
    #[uniform(2)]
    pub clip_map: [Vec4; 256],
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

// #[derive(AsBindGroup, Debug, Clone, TypeUuid)]
// #[uuid = "4489d09d-0685-438e-9aa4-1ecae35eb878"]
// pub struct ChunkClipHeightMaterial {
//     #[texture(0)]
//     #[sampler(1)]
//     pub texture: Handle<Image>,
//     #[uniform(2)]
//     pub tile_texture_size: f32,
//     #[uniform(2)]
//     pub clip_height: f32,
// }

// impl ChunkClipHeightMaterial {
//     pub const ATTRIBUTE_TILE_COORD_START: MeshVertexAttribute =
//         MeshVertexAttribute::new("TileCoordStart", 4789, VertexFormat::Float32x2);

//     pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
//         MeshVertexAttribute::new("Light", 4790, VertexFormat::Float32x3);

//     pub const ATTRIBUTE_OCCLUSION: MeshVertexAttribute =
//         MeshVertexAttribute::new("Occlusion", 4791, VertexFormat::Uint8x4);
// }

// impl Material for ChunkClipHeightMaterial {
//     fn fragment_shader() -> ShaderRef {
//         "shaders/voxel.wgsl".into()
//     }

//     fn vertex_shader() -> ShaderRef {
//         "shaders/voxel.wgsl".into()
//     }

//     fn specialize(
//         _pipeline: &MaterialPipeline<Self>,
//         descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
//         layout: &bevy::render::mesh::MeshVertexBufferLayout,
//         _key: MaterialPipelineKey<Self>,
//     ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
//         let vertex_layout = layout.get_layout(&[
//             Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
//             Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
//             Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
//             ChunkClipHeightMaterial::ATTRIBUTE_TILE_COORD_START.at_shader_location(3),
//             ChunkClipHeightMaterial::ATTRIBUTE_LIGHT.at_shader_location(4),
//             ChunkClipHeightMaterial::ATTRIBUTE_OCCLUSION.at_shader_location(5),
//         ])?;
//         descriptor.vertex.buffers = vec![vertex_layout];

//         descriptor
//             .vertex
//             .shader_defs
//             .push("CLIP_HEIGHT".to_string());

//         Ok(())
//     }
// }
