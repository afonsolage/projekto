use bevy::pbr::MaterialPipelineKey;
use bevy::prelude::*;

use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    encase, AsBindGroup, AsBindGroupError, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BufferBindingType,
    BufferInitDescriptor, BufferUsages, OwnedBindingResource, PreparedBindGroup,
    SamplerBindingType, ShaderRef, ShaderStages, ShaderType, TextureSampleType,
    TextureViewDimension, VertexFormat,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::FallbackImage;
use bevy::{pbr::MaterialPipeline, reflect::TypeUuid, render::mesh::MeshVertexAttribute};

#[derive(Reflect, Component, Debug, Deref, DerefMut)]
pub struct ChunkMaterialHandle(pub Handle<ChunkMaterial>);

#[derive(Debug, Clone, TypeUuid)]
#[uuid = "f690fd1e-d5d8-45ab-8225-97e2a3f056e0"]
pub struct ChunkMaterial {
    // #[texture(0)]
    // #[sampler(1)]
    pub texture: Handle<Image>,
    // #[uniform(2)]
    pub tile_texture_size: f32,
    // #[uniform(2)]
    pub clip_map_origin: Vec2,
    // #[uniform(2)]
    pub clip_height: f32,
    // #[texture(3)]
    pub clip_map: Handle<Image>,
}

#[derive(ShaderType)]
struct ChunkMaterialUniform {
    tile_texture_size: f32,
    clip_map_origin: Vec2,
    clip_height: f32,
}

impl From<&ChunkMaterial> for ChunkMaterialUniform {
    fn from(mat: &ChunkMaterial) -> Self {
        Self {
            tile_texture_size: mat.tile_texture_size,
            clip_map_origin: mat.clip_map_origin,
            clip_height: mat.clip_height,
        }
    }
}

impl ChunkMaterial {
    pub const ATTRIBUTE_TILE_COORD_START: MeshVertexAttribute =
        MeshVertexAttribute::new("TileCoordStart", 66438, VertexFormat::Float32x2);

    pub const ATTRIBUTE_LIGHT: MeshVertexAttribute =
        MeshVertexAttribute::new("Light", 66439, VertexFormat::Float32x3);

    pub const ATTRIBUTE_VOXEL: MeshVertexAttribute =
        MeshVertexAttribute::new("Voxel", 66440, VertexFormat::Uint32);
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
        layout: &bevy::render::mesh::MeshVertexBufferLayout,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        let vertex_layout = layout.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            ChunkMaterial::ATTRIBUTE_TILE_COORD_START.at_shader_location(3),
            ChunkMaterial::ATTRIBUTE_LIGHT.at_shader_location(4),
            ChunkMaterial::ATTRIBUTE_VOXEL.at_shader_location(5),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

impl AsBindGroup for ChunkMaterial {
    type Data = ();

    fn as_bind_group(
        &self,
        layout: &BindGroupLayout,
        render_device: &RenderDevice,
        images: &RenderAssets<Image>,
        _: &FallbackImage,
    ) -> Result<PreparedBindGroup<Self>, AsBindGroupError> {
        let texture = images
            .get(&self.texture)
            .ok_or(AsBindGroupError::RetryNextUpdate)?;
        let clip_map = images
            .get(&self.clip_map)
            .ok_or(AsBindGroupError::RetryNextUpdate)?;

        let mut buffer = encase::UniformBuffer::new(vec![]);
        buffer.write(&ChunkMaterialUniform::from(self)).unwrap();

        let bindings = vec![
            OwnedBindingResource::TextureView(texture.texture_view.clone()),
            OwnedBindingResource::Sampler(texture.sampler.clone()),
            OwnedBindingResource::Buffer(render_device.create_buffer_with_data(
                &BufferInitDescriptor {
                    label: None,
                    contents: buffer.as_ref(),
                    usage: BufferUsages::COPY_DST | BufferUsages::UNIFORM,
                },
            )),
            OwnedBindingResource::TextureView(clip_map.texture_view.clone()),
        ];

        let bind_group = render_device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: bindings[0].get_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: bindings[1].get_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: bindings[2].get_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: bindings[3].get_binding(),
                },
            ],
        });

        Ok(PreparedBindGroup {
            bindings,
            bind_group,
            data: (),
        })
    }

    fn bind_group_layout(render_device: &RenderDevice) -> BindGroupLayout {
        render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(<ChunkMaterialUniform as ShaderType>::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Uint,
                        view_dimension: TextureViewDimension::D1,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        })
    }
}
