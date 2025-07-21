use std::time::Duration;

use asset::ChunkAssetPlugin;
use bevy::{prelude::*, time::common_conditions::on_timer};
use net::NetPlugin;

// pub mod app;
mod light;
mod meshing;

mod asset;

pub use asset::{setup_chunk_asset_loader, ChunkAsset};

mod net;

pub mod cache;
pub mod gen;

pub mod bundle;
pub mod set;

const MESHING_TICK_MS: u64 = 500;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(PreUpdate, WorldSet::ReceiveRequests)
            .configure_sets(
                Update,
                (
                    WorldSet::LandscapeUpdate,
                    WorldSet::ChunkManagement,
                    WorldSet::Propagation,
                    WorldSet::Meshing.run_if(on_timer(Duration::from_millis(MESHING_TICK_MS))),
                )
                    .chain(),
            )
            .configure_sets(PostUpdate, WorldSet::SendResponses)
            .add_plugins((
                ChunkAssetPlugin,
                NetPlugin,
                set::LandscapePlugin,
                set::ChunkManagementPlugin,
                // set::ChunkInitializationPlugin,
                set::PropagationPlugin,
                set::MeshingPlugin,
                set::SendResponsesPlugin,
                set::ReceiveRequestsPlugin,
            ));
    }
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum WorldSet {
    ReceiveRequests,
    LandscapeUpdate,
    ChunkManagement,
    ChunkInitialization,
    Propagation,
    Meshing,
    SendResponses,
}

#[cfg(test)]
mod test {
    use bevy::app::ScheduleRunnerPlugin;

    use super::*;

    #[test]
    fn plugin() {
        App::new()
            .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(AssetPlugin::default())
            .add_plugins(super::WorldServerPlugin)
            .run();
    }
}
