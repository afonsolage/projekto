use bevy::prelude::*;
use orbit::OrbitCameraPlugin;

use self::fly_by::FlyByCameraPlugin;

pub mod fly_by;
pub mod orbit;

/// This is a wrapper plugin which justs adds [`FlyByCameraPlugin`] and [`OrbitCameraPlugin`]
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(FlyByCameraPlugin)
            .add_plugin(OrbitCameraPlugin);
    }
}
