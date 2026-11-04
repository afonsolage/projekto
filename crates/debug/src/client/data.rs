use bevy::prelude::*;
use projekto_client::ChunkMeshBuilt;

use crate::{ChunkDebug, ChunkDebugSource};

/// Collects client debug data only when the debug companion crate is enabled.
pub struct DebugDataPlugin;

impl Plugin for DebugDataPlugin {
    fn build(&self, app: &mut App) {
        crate::register_types(app);
        app.add_systems(PostUpdate, sync_debug);
    }
}

fn sync_debug(
    mut commands: Commands,
    mut mesh_built: MessageReader<ChunkMeshBuilt>,
    mut q_debug: Query<&mut ChunkDebug>,
) {
    for built in mesh_built.read() {
        let debug = q_debug.get_mut(built.entity);

        if let Ok(mut debug) = debug {
            debug.set_mesh(built.vertex_count, false);
            debug.top_y = built.top_y;
            debug.center_y = built.center_y;
        } else {
            let mut debug =
                ChunkDebug::new(built.chunk.x, built.chunk.z, ChunkDebugSource::Streamed);
            debug.set_mesh(built.vertex_count, false);
            debug.top_y = built.top_y;
            debug.center_y = built.center_y;
            commands.entity(built.entity).try_insert(debug);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;
    use bevy::ecs::message::Messages;
    use projekto_core::coords::Chunk;

    use super::*;

    #[test]
    fn mesh_built_for_despawned_entity_does_not_panic() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_message::<ChunkMeshBuilt>()
            .add_plugins(DebugDataPlugin);

        let entity = app.world_mut().spawn_empty().id();
        app.world_mut().despawn(entity);
        app.world_mut()
            .resource_mut::<Messages<ChunkMeshBuilt>>()
            .write(ChunkMeshBuilt {
                entity,
                chunk: Chunk::new(0, 0),
                vertex_count: 4,
                top_y: 1.0,
                center_y: 1.0,
            });

        app.update();
    }
}
