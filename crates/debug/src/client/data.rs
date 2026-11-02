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
            commands.entity(built.entity).insert(debug);
        }
    }
}
