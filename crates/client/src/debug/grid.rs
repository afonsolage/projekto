use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::shader::ShaderRef;
use bevy::{pbr::MaterialPipelineKey, prelude::*};

use bevy::{
    pbr::MaterialPipeline,
    render::render_resource::{AsBindGroup, Face, ShaderType},
};
use projekto_core::voxel;

use projekto_debug::DebugViewConfig;

use crate::{ChunkMaterialHandle, KindsAtlasRes, bundle::ChunkLocal, material::ChunkMaterial};

#[derive(Reflect, AsBindGroup, Asset, Debug, Clone)]
#[bind_group_data(bool)]
pub struct DebugChunkMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    #[uniform(2)]
    pub data: DebugChunkMaterialData,

    pub show_back_faces: bool,
}

#[derive(Reflect, ShaderType, Debug, Clone)]
pub struct DebugChunkMaterialData {
    pub tile_texture_size: f32,
    pub debug_chunk_grid: f32,
    pub _padding: Vec2,
}

impl From<&DebugChunkMaterial> for bool {
    fn from(value: &DebugChunkMaterial) -> Self {
        value.show_back_faces
    }
}

impl Material for DebugChunkMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/voxel_debug.wesl".into()
    }

    fn vertex_shader() -> ShaderRef {
        "shaders/voxel_debug.wesl".into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
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

        if _key.bind_group_data {
            descriptor.primitive.cull_mode = None;
        } else {
            descriptor.primitive.cull_mode = Some(Face::Back);
        }

        Ok(())
    }
}

#[derive(Resource)]
struct DebugGridMaterials(Handle<DebugChunkMaterial>);

pub struct DebugGridPlugin;

impl Plugin for DebugGridPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugViewConfig>()
            .add_plugins(MaterialPlugin::<DebugChunkMaterial>::default())
            .add_systems(Startup, setup_debug_material)
            .add_systems(PostUpdate, swap_grid_material);
    }
}

fn setup_debug_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<DebugChunkMaterial>>,
    kinds_res: Res<KindsAtlasRes>,
) {
    let material = materials.add(DebugChunkMaterial {
        texture: kinds_res.atlas.clone(),
        data: DebugChunkMaterialData {
            tile_texture_size: 1.0 / voxel::KindsDescs::get().count_tiles() as f32,
            debug_chunk_grid: 1.0,
            _padding: Vec2::ZERO,
        },
        show_back_faces: false,
    });

    commands.insert_resource(DebugGridMaterials(material));
}

type ChunkMaterialQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static MeshMaterial3d<ChunkMaterial>>,
        Option<&'static MeshMaterial3d<DebugChunkMaterial>>,
    ),
    With<ChunkLocal>,
>;

fn swap_grid_material(
    config: Option<Res<DebugViewConfig>>,
    normal: Res<ChunkMaterialHandle>,
    debug: Option<Res<DebugGridMaterials>>,
    mut commands: Commands,
    q: ChunkMaterialQuery,
) {
    let Some(config) = config else {
        return;
    };
    let Some(debug) = debug else {
        return;
    };

    for (entity, normal_material, debug_material) in &q {
        if config.show_grid {
            if normal_material.is_some() {
                commands
                    .entity(entity)
                    .remove::<MeshMaterial3d<ChunkMaterial>>()
                    .insert(MeshMaterial3d(debug.0.clone()));
            }
        } else if debug_material.is_some() {
            commands
                .entity(entity)
                .remove::<MeshMaterial3d<DebugChunkMaterial>>()
                .insert(MeshMaterial3d(normal.0.clone()));
        }
    }
}
