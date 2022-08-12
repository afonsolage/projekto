use bevy::prelude::*;

use self::fly_by::FlyByCameraPlugin;

pub mod birds_eye;
pub mod fly_by;


#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct MainCamera;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(FlyByCameraPlugin);
    }
}