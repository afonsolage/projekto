//! Windowless, low-fps client used by AI-driven diagnostics.

use std::{fs, path::PathBuf};

use bevy::{
    asset::RenderAssetUsages,
    camera::RenderTarget,
    prelude::*,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, save_to_disk},
    },
};

const AI_WIDTH: u32 = 960;
const AI_HEIGHT: u32 = 540;
const SNAPSHOT_INTERVAL_SECS: f32 = 5.0;

pub struct AiClientPlugin;

impl Plugin for AiClientPlugin {
    fn build(&self, app: &mut App) {
        let mut config = app
            .world_mut()
            .get_resource_or_insert_with(crate::DebugViewConfig::default);
        config.show_grid = true;
        config.show_labels = true;

        app.init_resource::<AiSnapshot>()
            .add_systems(PostStartup, setup_render_target)
            .add_systems(Update, (queue_periodic_snapshot, capture_snapshot).chain());
    }
}

#[derive(Resource, Default)]
pub(crate) struct AiSnapshot(pub(crate) bool);

#[derive(Resource)]
pub(crate) struct AiTarget(pub(crate) Handle<Image>);

fn setup_render_target(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut targets: Query<&mut RenderTarget>,
) {
    let mut image =
        Image::new_target_texture(AI_WIDTH, AI_HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
    image.asset_usage = RenderAssetUsages::RENDER_WORLD;
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;

    let handle = images.add(image);
    let target: RenderTarget = handle.clone().into();

    for mut render_target in &mut targets {
        *render_target = target.clone();
    }

    info!("AI client rendering offscreen at {AI_WIDTH}x{AI_HEIGHT}, snapshots in debug_reports/ai");

    commands.insert_resource(AiTarget(handle));
}

fn queue_periodic_snapshot(
    time: Res<Time>,
    mut request: ResMut<AiSnapshot>,
    mut timer: Local<Option<Timer>>,
) {
    let timer = timer
        .get_or_insert_with(|| Timer::from_seconds(SNAPSHOT_INTERVAL_SECS, TimerMode::Repeating));

    if timer.tick(time.delta()).just_finished() {
        request.0 = true;
    }
}

fn capture_snapshot(
    mut request: ResMut<AiSnapshot>,
    target: Option<Res<AiTarget>>,
    mut commands: Commands,
) {
    if !request.0 {
        return;
    }
    request.0 = false;

    let Some(target) = target else {
        return;
    };
    let Some(path) = snapshot_path() else {
        warn!("cannot resolve AI snapshot path");
        return;
    };

    info!("capturing AI snapshot to {}", path.display());
    commands
        .spawn(Screenshot::image(target.0.clone()))
        .observe(save_to_disk(path));
}

fn snapshot_path() -> Option<PathBuf> {
    let root = std::env::var("BEVY_ASSET_ROOT").ok()?;
    let dir = PathBuf::from(root).join("debug_reports").join("ai");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("latest.png"))
}
