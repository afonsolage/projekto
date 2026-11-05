use crate::{ChunkDebug, ChunkDebugSource, ChunkDebugState, center_height};
use bevy::prelude::*;

use projekto_server::{
    DirtyChunks,
    bundle::{CachedMesh, ChunkKind, ChunkLocal, ChunkMap, ChunkVertex},
    set::MeshTasks,
};

/// Collects chunk lifecycle data only when the debug-tools feature is enabled.
pub struct DebugDataPlugin;

impl Plugin for DebugDataPlugin {
    fn build(&self, app: &mut App) {
        crate::register_types(app);
        app.add_systems(PostUpdate, (sync_debug, refresh_debug).chain());
    }
}

type DebugQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChunkLocal,
        &'static ChunkVertex,
        &'static ChunkKind,
        Option<&'static mut ChunkDebug>,
        Option<&'static CachedMesh>,
    ),
    Or<(Added<ChunkLocal>, Changed<ChunkVertex>)>,
>;

fn sync_debug(mut commands: Commands, mut q: DebugQuery) {
    for (entity, local, vertex, kind, debug, cached) in &mut q {
        let source = if cached.is_some() {
            ChunkDebugSource::Archive
        } else {
            ChunkDebugSource::Generated
        };
        let vertex_count = vertex.0.len() as u32;
        let top_y = vertex
            .0
            .iter()
            .map(|vertex| vertex.position.y)
            .fold(f32::MIN, f32::max);
        let center_y =
            center_height(vertex.0.iter().map(|vertex| vertex.position)).unwrap_or(top_y);

        let Some(mut debug) = debug else {
            let mut debug = ChunkDebug::new(local.0.x, local.0.z, source);
            // A generated chunk starts with an empty buffer before its first mesh.
            // Keep it in Loading until the meshing pipeline produces a result.
            if cached.is_some() || vertex_count > 0 {
                let suspect_hole = vertex_count == 0 && !kind.0.all(|kind| kind.is_none());
                debug.set_mesh(vertex_count, suspect_hole);
                debug.top_y = if vertex_count == 0 { 0.0 } else { top_y };
                debug.center_y = if vertex_count == 0 { 0.0 } else { center_y };
            }
            commands.entity(entity).insert(debug);
            continue;
        };

        let suspect_hole = vertex_count == 0 && !kind.0.all(|kind| kind.is_none());
        debug.set_mesh(vertex_count, suspect_hole);
        debug.top_y = if vertex_count == 0 { 0.0 } else { top_y };
        debug.center_y = if vertex_count == 0 { 0.0 } else { center_y };
        if suspect_hole {
            warn!(
                "chunk ({}, {}) produced an empty mesh over solid terrain",
                local.0.x, local.0.z
            );
        }
    }
}

fn refresh_debug(
    time: Res<Time>,
    chunk_map: Res<ChunkMap>,
    dirty: Res<DirtyChunks>,
    tasks: Option<Res<MeshTasks>>,
    mut q: Query<(&ChunkLocal, &ChunkVertex, &ChunkKind, &mut ChunkDebug)>,
    mut timer: Local<Option<Timer>>,
) {
    let timer = timer.get_or_insert_with(|| Timer::from_seconds(0.5, TimerMode::Repeating));
    if !timer.tick(time.delta()).just_finished() {
        return;
    }

    for (local, vertex, kind, mut debug) in &mut q {
        if debug.state == ChunkDebugState::Loading
            && !dirty.0.contains(&local.0)
            && !tasks
                .as_ref()
                .is_some_and(|tasks| tasks.is_pending(local.0))
        {
            let vertex_count = vertex.0.len() as u32;
            let top_y = vertex
                .0
                .iter()
                .map(|vertex| vertex.position.y)
                .fold(f32::MIN, f32::max);
            let center_y =
                center_height(vertex.0.iter().map(|vertex| vertex.position)).unwrap_or(top_y);
            let suspect_hole = vertex_count == 0 && !kind.0.all(|kind| kind.is_none());
            debug.set_mesh(vertex_count, suspect_hole);
            debug.top_y = if vertex_count == 0 { 0.0 } else { top_y };
            debug.center_y = if vertex_count == 0 { 0.0 } else { center_y };
            if suspect_hole {
                warn!(
                    "chunk ({}, {}) produced an empty mesh over solid terrain",
                    local.0.x, local.0.z
                );
            }
        }

        let mut mask = 0u8;
        let mut bit = 0u8;

        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                if chunk_map.contains_key(&projekto_core::coords::Chunk::new(
                    local.0.x + dx,
                    local.0.z + dz,
                )) {
                    mask |= 1 << bit;
                }
                bit += 1;
            }
        }

        debug.neighbors_loaded = mask;
        debug.dirty = dirty.0.contains(&local.0);
    }
}
