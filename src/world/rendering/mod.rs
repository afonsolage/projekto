use bevy::{prelude::*, utils::HashMap};

use self::{landscaping::LandscapingPlugin, meshing::MeshingPlugin};

pub use landscaping::LandscapeConfig;

mod landscaping;
mod material;
mod meshing;

pub use material::{ChunkMaterial, ChunkMaterialHandle};

#[derive(Component)]
pub struct LandscapeCenter;

pub struct PipelinePlugin;
impl Plugin for PipelinePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ChunkMaterial>()
            .add_plugins((LandscapingPlugin, MeshingPlugin));
    }
}

/// This event is raised whenever a chunk mesh needs to be redrawn
#[derive(Event)]
pub struct EvtChunkMeshDirty(pub IVec3);

#[derive(Component)]
pub struct ChunkLocal(pub IVec3);

#[derive(Resource, Deref, DerefMut)]
pub struct ChunkEntityMap(pub HashMap<IVec3, Entity>);

#[derive(Bundle)]
pub struct ChunkBundle {
    local: ChunkLocal,
    mesh_bundle: MaterialMeshBundle<material::ChunkMaterial>,
}

impl Default for ChunkBundle {
    fn default() -> Self {
        Self {
            local: ChunkLocal(IVec3::ZERO),
            mesh_bundle: MaterialMeshBundle::default(),
        }
    }
}
