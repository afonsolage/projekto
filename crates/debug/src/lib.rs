//! Lightweight, reflected debug data shared by the server and the client.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Stable chunk identifier derived from its coordinates.
pub fn chunk_id(x: i32, z: i32) -> u64 {
    ((x as u32 as u64) << 32) | (z as u32 as u64)
}

/// Deterministic colour for a chunk, useful for screenshots.
pub fn chunk_color(id: u64) -> Color {
    let hash = id.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let hue = ((hash >> 40) & 0xFFFF) as f32 / 65_535.0 * 360.0;
    Color::hsl(hue, 0.7, 0.55)
}

#[derive(Reflect, Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChunkDebugState {
    #[default]
    Loading,
    Meshing,
    Ready,
    Empty,
    Failed,
}

#[derive(Reflect, Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChunkDebugSource {
    #[default]
    Generated,
    Archive,
    Streamed,
}

/// Compact debug summary attached to each chunk entity.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone)]
#[reflect(Component, Serialize, Deserialize)]
pub struct ChunkDebug {
    pub id: u64,
    pub x: i32,
    pub z: i32,
    pub state: ChunkDebugState,
    pub source: ChunkDebugSource,
    pub vertex_count: u32,
    pub face_count: u32,
    pub suspect_hole: bool,
    pub neighbors_loaded: u8,
    pub dirty: bool,
    pub saved: bool,
}

impl ChunkDebug {
    pub fn new(x: i32, z: i32, source: ChunkDebugSource) -> Self {
        Self {
            id: chunk_id(x, z),
            x,
            z,
            state: ChunkDebugState::Loading,
            source,
            vertex_count: 0,
            face_count: 0,
            suspect_hole: false,
            neighbors_loaded: 0,
            dirty: true,
            saved: false,
        }
    }

    pub fn set_mesh(&mut self, vertex_count: u32, suspect_hole: bool) {
        self.vertex_count = vertex_count;
        self.face_count = vertex_count / 4;
        self.suspect_hole = suspect_hole;
        self.state = if vertex_count == 0 {
            ChunkDebugState::Empty
        } else {
            ChunkDebugState::Ready
        };
        self.dirty = false;
    }

    pub fn mark_saved(&mut self) {
        self.saved = true;
    }
}

/// A user-placed marker, queryable through BRP and visible in the world.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone)]
#[reflect(Component, Serialize, Deserialize)]
pub struct DebugPin {
    pub id: u64,
    pub x: i32,
    pub z: i32,
    pub position: Vec3,
    pub label: String,
}

/// Client-side visual debug switches.
#[derive(Resource, Reflect, Serialize, Deserialize, Debug, Clone)]
#[reflect(Resource, Serialize, Deserialize)]
pub struct DebugViewConfig {
    pub show_grid: bool,
    pub show_labels: bool,
    pub show_pins: bool,
    pub grid_height: f32,
}

impl Default for DebugViewConfig {
    fn default() -> Self {
        Self {
            show_grid: false,
            show_labels: false,
            show_pins: true,
            grid_height: 250.0,
        }
    }
}

/// Register every reflected debug type used by BRP.
pub fn register_types(app: &mut App) {
    app.register_type::<ChunkDebug>()
        .register_type::<ChunkDebugState>()
        .register_type::<ChunkDebugSource>()
        .register_type::<DebugPin>()
        .register_type::<DebugViewConfig>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_ids_are_unique_for_signs() {
        assert_ne!(chunk_id(1, -1), chunk_id(-1, 1));
        assert_ne!(chunk_id(0, 0), chunk_id(0, 1));
        assert_eq!(chunk_id(42, -7), chunk_id(42, -7));
    }

    #[test]
    fn empty_mesh_on_solid_chunk_is_suspect() {
        let mut debug = ChunkDebug::new(1, 2, ChunkDebugSource::Archive);
        debug.set_mesh(0, true);
        assert_eq!(debug.state, ChunkDebugState::Empty);
        assert!(debug.suspect_hole);
    }

    #[test]
    fn non_empty_mesh_is_ready() {
        let mut debug = ChunkDebug::new(1, 2, ChunkDebugSource::Generated);
        debug.set_mesh(12, false);
        assert_eq!(debug.state, ChunkDebugState::Ready);
        assert_eq!(debug.face_count, 3);
        assert!(!debug.dirty);
    }
}
