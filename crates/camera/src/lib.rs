use bevy::prelude::*;
use birds_eye::BirdsEyeCameraPlugin;

use self::fly_by::FlyByCameraPlugin;

pub mod birds_eye;
pub mod fly_by;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(FlyByCameraPlugin)
            .add_plugin(BirdsEyeCameraPlugin);
    }
}
