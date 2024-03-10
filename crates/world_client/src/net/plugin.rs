use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, TaskPool},
};

pub(crate) struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        //
    }
}

fn connect_to_client(mut commands: Commands) {
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            //
        })
        .detach();
}
