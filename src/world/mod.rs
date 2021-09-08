use bevy::prelude::*;

//mod debug;
mod math;
mod mesh;
mod pipeline;

pub mod raycast;
pub mod storage;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(pipeline::PipelinePlugin);
    }
}
