use bevy::prelude::*;

mod debug;
mod math;
mod mesh;
pub mod pipeline;

pub mod terraformation;

pub mod query;
pub mod storage;

pub use debug::DebugCmd;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(pipeline::PipelinePlugin)
            .add_plugin(terraformation::TerraformationPlugin)
            .add_plugin(debug::WireframeDebugPlugin);
    }
}
