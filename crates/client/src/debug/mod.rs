use bevy::app::{PluginGroup, PluginGroupBuilder};

mod chunks;
mod data;
mod grid;
mod memory;
#[cfg(feature = "remote-debug")]
mod remote;

pub use chunks::DebugUiPlugin;
pub use data::DebugDataPlugin;
pub use grid::DebugGridPlugin;
pub use memory::MemoryOverlayPlugin;
#[cfg(feature = "remote-debug")]
pub use remote::RemoteDebugPlugin;

/// Debug-only plugins, composed as a normal Bevy plugin group.
pub struct DebugToolsPlugins;

impl PluginGroup for DebugToolsPlugins {
    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::start::<Self>()
            .add(DebugDataPlugin)
            .add(MemoryOverlayPlugin)
            .add(DebugUiPlugin)
            .add(DebugGridPlugin);

        #[cfg(feature = "remote-debug")]
        let builder = builder.add(RemoteDebugPlugin);

        builder
    }
}
