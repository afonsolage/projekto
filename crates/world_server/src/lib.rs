use std::time::Duration;

use bevy::{ecs::query::ReadOnlyWorldQuery, prelude::*, time::common_conditions::on_timer};
use projekto_core::{
    chunk::{Chunk, ChunkStorage},
    voxel::{self},
};
use set::{
    ChunkInitializationPlugin, ChunkManagementPlugin, LandscapePlugin, MeshingPlugin,
    PropagationPlugin,
};

pub mod app;
pub mod channel;
mod genesis;
mod light;
mod meshing;

pub mod chunk_map;
pub mod set;

const MESHING_TICK_MS: u64 = 500;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                WorldSet::LandscapeUpdate.before(WorldSet::ChunkManagement),
                WorldSet::ChunkManagement.before(WorldSet::FlushCommands),
                WorldSet::ChunkInitialization.after(WorldSet::FlushCommands),
                WorldSet::Propagation.after(WorldSet::ChunkInitialization),
                WorldSet::Meshing
                    .after(WorldSet::Propagation)
                    .run_if(on_timer(Duration::from_millis(MESHING_TICK_MS))),
            ),
        )
        .add_plugins((
            LandscapePlugin,
            ChunkManagementPlugin,
            ChunkInitializationPlugin,
            PropagationPlugin,
            MeshingPlugin,
        ))
        .add_systems(Update, (apply_deferred.in_set(WorldSet::FlushCommands),));
    }
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
enum WorldSet {
    LandscapeUpdate,
    ChunkManagement,
    FlushCommands,
    ChunkInitialization,
    Propagation,
    Meshing,
}

// Components
#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkKind(ChunkStorage<voxel::Kind>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkLight(ChunkStorage<voxel::Light>);

#[derive(Component, Default, Debug, Clone, Copy, Deref, DerefMut)]
pub struct ChunkLocal(Chunk);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesOcclusion(ChunkStorage<voxel::FacesOcclusion>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesSoftLight(ChunkStorage<voxel::FacesSoftLight>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
pub struct ChunkVertex(Vec<voxel::Vertex>);

#[derive(Bundle, Default)]
struct ChunkBundle {
    kind: ChunkKind,
    light: ChunkLight,
    local: ChunkLocal,
    occlusion: ChunkFacesOcclusion,
    soft_light: ChunkFacesSoftLight,
    vertex: ChunkVertex,
}

fn any_chunk<T: ReadOnlyWorldQuery>(q_changed_chunks: Query<(), (T, With<ChunkLocal>)>) -> bool {
    !q_changed_chunks.is_empty()
}

#[cfg(test)]
mod test {
    use bevy::app::ScheduleRunnerPlugin;

    use super::*;

    #[test]
    fn plugin() {
        App::new()
            .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin)
            .run()
    }
}

// TODO: Extract and render to check if its working.
