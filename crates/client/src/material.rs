use bevy::{pbr::MaterialPipelineKey, prelude::*};

use bevy::{
    pbr::MaterialPipeline,
    render::{
        mesh::MeshVertexAttribute,
        render_resource::{AsBindGroup, Face, ShaderRef, VertexFormat},
    },
};

#[derive(Reflect, AsBindGroup, Asset, Debug, Clone)]
#[bind_group_data(bool)]
pub struct ChunkMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    #[uniform(2)]
    pub tile_texture_size: f32,

    pub show_back_faces: bool,
}

impl From<&ChunkMaterial> for bool {
    fn from(value: &ChunkMaterial) -> Self {
        value.show_back_faces
    }
}

// #[derive(ShaderType)]
// struct ChunkMaterialUniform {
//     tile_texture_size: f32,
//     clip_map_origin: Vec2,
//     clip_height: f32,
// }

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

    // fn alpha_mode(&self) -> AlphaMode {
    //     AlphaMode::Opaque
    // }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            ChunkMaterial::ATTRIBUTE_TILE_COORD_START.at_shader_location(3),
            ChunkMaterial::ATTRIBUTE_LIGHT.at_shader_location(4),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];

        let show_back_face = _key.bind_group_data;

        if show_back_face {
            descriptor.primitive.cull_mode = None;
        } else {
            descriptor.primitive.cull_mode = Some(Face::Back);
        }

        Ok(())
    }
}
