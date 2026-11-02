use bevy::{mesh::VertexAttributeValues, prelude::*};
use projekto_debug::{ChunkDebug, ChunkDebugSource};

use crate::bundle::ChunkLocal;

/// Collects mesh-derived debug data only when the debug-tools feature is enabled.
pub struct DebugDataPlugin;

impl Plugin for DebugDataPlugin {
    fn build(&self, app: &mut App) {
        projekto_debug::register_types(app);
        app.add_systems(PostUpdate, sync_debug);
    }
}

type DebugQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChunkLocal,
        &'static Mesh3d,
        Option<&'static mut ChunkDebug>,
    ),
    Or<(Added<ChunkLocal>, Changed<Mesh3d>)>,
>;

fn sync_debug(mut commands: Commands, meshes: Res<Assets<Mesh>>, mut q: DebugQuery) {
    const CENTER: f32 = 8.0;
    const RADIUS: f32 = 2.0;

    for (entity, local, mesh_handle, debug) in &mut q {
        let Some(mesh) = meshes.get(&mesh_handle.0) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };

        let vertex_count = positions.len() as u32;
        let top_y = positions
            .iter()
            .map(|position| position[1])
            .fold(f32::MIN, f32::max);
        let center_y = positions
            .iter()
            .filter(|position| {
                (position[0] - CENTER).abs() <= RADIUS && (position[2] - CENTER).abs() <= RADIUS
            })
            .map(|position| position[1])
            .reduce(f32::max)
            .unwrap_or(top_y);

        let Some(mut debug) = debug else {
            let mut debug = ChunkDebug::new(local.0.x, local.0.z, ChunkDebugSource::Streamed);
            debug.set_mesh(vertex_count, false);
            debug.top_y = if vertex_count == 0 { 0.0 } else { top_y };
            debug.center_y = if vertex_count == 0 { 0.0 } else { center_y };
            commands.entity(entity).insert(debug);
            continue;
        };

        debug.set_mesh(vertex_count, false);
        debug.top_y = if vertex_count == 0 { 0.0 } else { top_y };
        debug.center_y = if vertex_count == 0 { 0.0 } else { center_y };
    }
}
