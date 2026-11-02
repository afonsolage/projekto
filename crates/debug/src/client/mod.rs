use bevy::{
    app::{PluginGroup, PluginGroupBuilder},
    camera::visibility::RenderLayers,
    prelude::*,
};

mod chunks;
mod data;
mod grid;
mod memory;
mod remote;

pub use chunks::DebugUiPlugin;
pub use data::DebugDataPlugin;
pub use grid::DebugGridPlugin;
pub use memory::MemoryOverlayPlugin;
pub use remote::RemoteDebugPlugin;

/// Client debug plugins, composed as a normal Bevy plugin group.
pub struct ClientDebugPlugins {
    /// Port used by the client BRP server.
    pub remote_port: u16,
}

impl ClientDebugPlugins {
    /// Creates the client debug plugin group with BRP on `remote_port`.
    pub fn new(remote_port: u16) -> Self {
        Self { remote_port }
    }
}

impl PluginGroup for ClientDebugPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(DebugLayersPlugin)
            .add(DebugDataPlugin)
            .add(MemoryOverlayPlugin)
            .add(DebugUiPlugin)
            .add(DebugGridPlugin)
            .add(RemoteDebugPlugin::new(self.remote_port))
    }
}

struct DebugLayersPlugin;

impl Plugin for DebugLayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, configure_layers);
    }
}

fn configure_layers(
    mut commands: Commands,
    mut cameras_3d: Query<&mut RenderLayers, (With<Camera3d>, Without<Camera2d>)>,
    cameras_2d: Query<Entity, (With<Camera2d>, Without<Camera3d>)>,
) {
    for mut layers in &mut cameras_3d {
        *layers = RenderLayers::from_layers(&[0, 1, 2]);
    }

    for entity in &cameras_2d {
        commands.entity(entity).insert(RenderLayers::layer(3));
    }
}
