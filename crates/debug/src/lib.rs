//! Lightweight, reflected debug data shared by the server and the client.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub mod ai;
pub mod client;
pub mod server;

pub use ai::AiClientPlugin;
pub use client::ClientDebugPlugins;
pub use server::ServerDebugPlugins;

/// Stable chunk identifier derived from its coordinates.
pub fn chunk_id(x: i32, z: i32) -> u64 {
    ((x as u32 as u64) << 32) | (z as u32 as u64)
}

/// Deterministic colour for a chunk, useful for screenshots.
pub fn chunk_color(id: u64) -> Color {
    // The golden-ratio odd multiplier (`floor(2^64 / φ)`) mixes neighbouring
    // chunk ids into unrelated high bits, so adjacent chunks get distinct hues.
    let hash = id.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // Take a 16-bit slice from the well-mixed high bits, normalize it to
    // `0.0..=1.0` and map it to the hue circle in degrees.
    let hue = ((hash >> 40) & 0xFFFF) as f32 / 65_535.0 * 360.0;
    Color::hsl(hue, 0.7, 0.55)
}

/// Mesh lifecycle of a chunk, as tracked for the debug overlay.
#[derive(Reflect, Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChunkDebugState {
    /// Chunk data is still being generated or loaded.
    #[default]
    Loading,
    /// The chunk mesh is being built.
    Meshing,
    /// The chunk has a non-empty mesh.
    Ready,
    /// The chunk produced no mesh.
    Empty,
    /// The chunk could not be loaded or meshed.
    Failed,
}

/// Where a chunk's data came from.
#[derive(Reflect, Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChunkDebugSource {
    /// Generated from noise on the server.
    #[default]
    Generated,
    /// Loaded from the chunk archive.
    Archive,
    /// Received from the server by the client.
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
    /// Highest mesh vertex in chunk-local Y.
    pub top_y: f32,
    /// Highest mesh vertex near the chunk centre, used to keep labels on the local surface.
    pub center_y: f32,
    pub neighbors_loaded: u8,
    pub dirty: bool,
}

impl ChunkDebug {
    /// Creates a debug summary for the chunk at (`x`, `z`) in the `Loading` state.
    pub fn new(x: i32, z: i32, source: ChunkDebugSource) -> Self {
        Self {
            id: chunk_id(x, z),
            x,
            z,
            state: ChunkDebugState::Loading,
            source,
            vertex_count: 0,
            face_count: 0,
            top_y: 0.0,
            center_y: 0.0,
            neighbors_loaded: 0,
            dirty: true,
        }
    }

    /// Records the mesh result and moves the chunk to `Empty`, `Ready` or `Failed`.
    /// A zero-vertex mesh over solid terrain is reported as `Failed`.
    pub fn set_mesh(&mut self, vertex_count: u32, suspect_hole: bool) {
        self.vertex_count = vertex_count;
        self.face_count = vertex_count / 4;
        self.state = if suspect_hole {
            ChunkDebugState::Failed
        } else if vertex_count == 0 {
            ChunkDebugState::Empty
        } else {
            ChunkDebugState::Ready
        };
        self.dirty = false;
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
    /// World-space Y of the chunk grid plane, clamped below the camera at draw time.
    pub grid_height: f32,
}

impl Default for DebugViewConfig {
    fn default() -> Self {
        Self {
            show_grid: false,
            show_labels: false,
            show_pins: true,
            // Fixed default near the terrain surface (generated surfaces sit
            // roughly between Y=150 and Y=250) so the grid starts on land
            // instead of following the camera. `grid_y` still clamps it.
            grid_height: 224.0,
        }
    }
}

/// Returns the highest mesh vertex within two blocks of the chunk centre.
pub fn center_height(positions: impl Iterator<Item = Vec3>) -> Option<f32> {
    // Chunks are 16 voxels wide on X/Z, so the centre is at 8.0. A two-block
    // radius keeps the label on the local surface without sampling spikes that
    // may exist elsewhere in the chunk.
    const CENTER: f32 = 8.0;
    const RADIUS: f32 = 2.0;

    positions
        .filter(|position| {
            (position.x - CENTER).abs() <= RADIUS && (position.z - CENTER).abs() <= RADIUS
        })
        .map(|position| position.y)
        .reduce(f32::max)
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
    fn empty_mesh_on_solid_chunk_fails() {
        let mut debug = ChunkDebug::new(1, 2, ChunkDebugSource::Archive);
        debug.set_mesh(0, true);
        assert_eq!(debug.state, ChunkDebugState::Failed);
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
