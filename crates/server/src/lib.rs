use std::time::Duration;

use asset::ChunkAssetPlugin;
use bevy::{prelude::*, time::common_conditions::on_timer};
use net::NetPlugin;

// pub mod app;
pub mod light;
pub mod meshing;

pub mod archive;
mod asset;

pub use asset::{ChunkAsset, ChunkAssetHandle, setup_chunk_asset_loader};

pub mod debug;
mod net;

pub mod cache;
pub mod r#gen;

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
            .insert_resource(debug::Metrics::new())
            .add_plugins((
                ChunkAssetPlugin,
                NetPlugin,
                set::ReceiveRequestsPlugin,
                set::LandscapePlugin,
                set::ChunkManagementPlugin,
                // set::ChunkInitializationPlugin,
                set::PropagationPlugin,
                set::MeshingPlugin,
                set::SendResponsesPlugin,
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
