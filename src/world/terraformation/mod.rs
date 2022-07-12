use bevy::prelude::*;

mod genesis;

pub mod prelude {
    pub use super::genesis::BatchChunkCmdRes;
    pub use super::genesis::WorldRes;
    pub use super::genesis::EvtChunkLoaded;
    pub use super::genesis::EvtChunkUnloaded;
    pub use super::genesis::EvtChunkUpdated;
}

pub struct TerraformationPlugin;

impl Plugin for TerraformationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(genesis::GenesisPlugin);
    }
}
