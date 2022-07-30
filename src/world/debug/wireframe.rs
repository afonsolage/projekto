use bevy::{
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    reflect::TypeUuid,
    render::render_resource::{AsBindGroup, ShaderRef},
};

#[derive(AsBindGroup, Debug, Clone, TypeUuid)]
#[uuid = "f690fd1e-d5d8-45ab-8225-97e2a3f056e1"]
pub struct WireframeMaterial {
    #[uniform(0)]
    pub color: Color,
}

impl Material for WireframeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/wireframe.wgsl".into()
    }

    fn vertex_shader() -> ShaderRef {
        "shaders/wireframe.wgsl".into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::render::mesh::MeshVertexBufferLayout,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        let vertex_layout = layout.get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}
