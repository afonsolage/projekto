use bevy::{prelude::*, reflect::TypeUuid};
use projekto_core::voxel;

pub(crate) mod debug;
pub mod rendering;

pub mod terraformation;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(terraformation::TerraformationPlugin)
            .add_plugin(rendering::PipelinePlugin)
            .add_plugin(debug::WireframeDebugPlugin)
            .add_startup_system_to_stage(StartupStage::PreStartup, setup_resources);
    }
}

#[derive(TypeUuid, Debug)]
#[uuid = "e6edff2a-e204-497f-999c-bdebd1f92f62"]
pub struct KindsAtlasRes {
    pub atlas: Handle<Image>,
}

fn setup_resources(mut commands: Commands, asset_server: Res<AssetServer>) {
    let kinds_path = format!("{}{}", env!("ASSETS_PATH"), "/voxels/kind.ron");
    let descs = voxel::KindsDescs::init(kinds_path);

    let atlas = asset_server.load(&descs.atlas_path);

    commands.insert_resource(KindsAtlasRes { atlas });
}
