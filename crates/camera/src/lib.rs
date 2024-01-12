#![allow(clippy::type_complexity)]
use bevy::prelude::*;
use orbit::OrbitCameraPlugin;

use self::fly_by::FlyByCameraPlugin;

pub mod fly_by;
pub mod orbit;

/// This is a wrapper plugin which justs adds [`FlyByCameraPlugin`] and [`OrbitCameraPlugin`]
pub struct CameraPlugin;

/// [`SystemLabel`] used by internals systems.
#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub struct CameraUpdate;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FlyByCameraPlugin, OrbitCameraPlugin));
    }
}
