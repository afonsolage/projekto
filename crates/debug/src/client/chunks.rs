use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{ChunkDebug, DebugPin, DebugViewConfig, chunk_id, client::memory::ServerMemory};
use bevy::{
    camera::visibility::RenderLayers,
    math::Isometry3d,
    picking::events::{PointerOut, PointerOver},
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use projekto_client::{ChunkLocal, ChunkMap};
use projekto_core::coords::Chunk;
use projekto_diagnostics::ProcessMemory;
use serde::Serialize;

const LABEL_RADIUS: i32 = 3;
const OVERLAY_REFRESH_SECS: f32 = 0.25;

pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin);

        app.init_resource::<DebugViewConfig>()
            .init_resource::<HoveredChunk>()
            .init_resource::<ReportRequest>()
            .add_observer(on_pointer_over)
            .add_observer(on_pointer_out)
            .add_systems(Startup, (configure_gizmos, spawn_debug_overlay))
            .add_systems(
                Update,
                (
                    toggle_debug_views,
                    pin_hovered_chunk,
                    request_report,
                    draw_labels,
                    draw_pins,
                    update_debug_overlay,
                    capture_report,
                ),
            );
    }
}

#[derive(Resource, Default)]
pub(crate) struct HoveredChunk {
    pub(crate) entity: Option<Entity>,
    pub(crate) chunk: Option<Chunk>,
    pub(crate) position: Vec3,
}

#[derive(Resource, Default)]
pub(crate) struct ReportRequest(pub(crate) bool);

#[derive(Component)]
struct DebugOverlay;

fn on_pointer_over(
    over: On<PointerOver>,
    q_chunks: Query<&ChunkLocal>,
    mut hovered: ResMut<HoveredChunk>,
) {
    let event = over.event();

    if let Ok(local) = q_chunks.get(event.entity) {
        hovered.entity = Some(event.entity);
        hovered.chunk = Some(local.0);
        hovered.position = event.hit.position.unwrap_or(Vec3::ZERO);
    }
}

fn on_pointer_out(out: On<PointerOut>, mut hovered: ResMut<HoveredChunk>) {
    if hovered.entity == Some(out.event().entity) {
        hovered.entity = None;
        hovered.chunk = None;
    }
}

fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    if let Some((config, _)) = store.get_config_mut::<DefaultGizmoConfigGroup>() {
        // Let terrain occlude world-space labels and pins instead of drawing them
        // through hills and caves.
        config.depth_bias = 0.0;
        config.line.width = 3.0;
        // The memory overlay uses a Camera2d. Keep gizmos out of that 2D pass;
        // otherwise labels are also drawn once in screen space.
        config.render_layers = RenderLayers::layer(2);
    }
}

fn toggle_debug_views(
    keys: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<DebugViewConfig>,
    pins: Query<Entity, With<DebugPin>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::F4) {
        config.show_grid = !config.show_grid;
        info!("chunk grid: {}", config.show_grid);
    }

    if keys.just_pressed(KeyCode::F5) {
        config.show_labels = !config.show_labels;
        info!("chunk labels: {}", config.show_labels);
    }

    if keys.just_pressed(KeyCode::F9) {
        for entity in &pins {
            commands.entity(entity).despawn();
        }
        info!("debug pins cleared");
    }
}

fn pin_hovered_chunk(
    keys: Res<ButtonInput<KeyCode>>,
    hovered: Res<HoveredChunk>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::F8) {
        return;
    }

    let Some(chunk) = hovered.chunk else {
        return;
    };
    let position = hovered.position;

    info!("debug pin at chunk ({}, {})", chunk.x, chunk.z);
    spawn_pin(&mut commands, chunk, position);
}

pub(crate) fn spawn_pin(commands: &mut Commands, chunk: Chunk, position: Vec3) {
    commands.spawn(DebugPin {
        id: chunk_id(chunk.x, chunk.z),
        x: chunk.x,
        z: chunk.z,
        position,
        label: format!("PIN ({}, {})", chunk.x, chunk.z),
    });
}

fn request_report(keys: Res<ButtonInput<KeyCode>>, mut request: ResMut<ReportRequest>) {
    if keys.just_pressed(KeyCode::F7) {
        request.0 = true;
    }
}

fn active_camera_transform(
    cameras: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Option<(Vec3, Vec3)> {
    cameras
        .iter()
        .find(|(camera, _)| camera.is_active)
        .map(|(_, transform)| (transform.translation(), transform.forward().as_vec3()))
}

fn draw_pins(mut gizmos: Gizmos, config: Res<DebugViewConfig>, pins: Query<&DebugPin>) {
    if !config.show_pins {
        return;
    }

    for pin in &pins {
        let color = Color::srgb(1.0, 0.2, 1.0);
        gizmos.line(pin.position, pin.position + Vec3::Y * 6.0, color);
        gizmos.line(
            pin.position + Vec3::X * 1.5,
            pin.position - Vec3::X * 1.5,
            color,
        );
        gizmos.line(
            pin.position + Vec3::Z * 1.5,
            pin.position - Vec3::Z * 1.5,
            color,
        );

        gizmos.text(
            Isometry3d::from_translation(pin.position + Vec3::Y * 2.0),
            &pin.label,
            4.0,
            Vec2::ZERO,
            color,
        );
    }
}

pub(crate) fn grid_y(config: &DebugViewConfig, camera_position: Vec3) -> f32 {
    config.grid_height.min(camera_position.y - 16.0)
}

fn draw_labels(
    mut gizmos: Gizmos,
    config: Res<DebugViewConfig>,
    chunk_map: Res<ChunkMap>,
    q_debug: Query<&ChunkDebug>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    if !config.show_labels {
        return;
    }

    let Some((camera_position, _)) = active_camera_transform(&cameras) else {
        return;
    };

    let center = Chunk::from(camera_position);

    for dx in -LABEL_RADIUS..=LABEL_RADIUS {
        for dz in -LABEL_RADIUS..=LABEL_RADIUS {
            let chunk = Chunk::new(center.x + dx, center.z + dz);
            let Some(&entity) = chunk_map.get(&chunk) else {
                continue;
            };
            let Ok(debug) = q_debug.get(entity) else {
                continue;
            };
            if debug.vertex_count == 0 {
                continue;
            }

            let position = Vec3::new(
                chunk.x as f32 * Chunk::X_AXIS_SIZE as f32 + Chunk::X_AXIS_SIZE as f32 / 2.0,
                debug.center_y + 1.5,
                chunk.z as f32 * Chunk::Z_AXIS_SIZE as f32 + Chunk::Z_AXIS_SIZE as f32 / 2.0,
            );
            let direction = (camera_position - position).normalize_or_zero();
            let rotation = Quat::from_rotation_arc(Vec3::Z, direction);
            let distance = camera_position.distance(position);
            let font_size = (distance / 32.0).clamp(0.3, 0.7);

            gizmos.text(
                Isometry3d::new(position, rotation),
                &format!("({}, {})", chunk.x, chunk.z),
                font_size,
                Vec2::ZERO,
                Color::WHITE,
            );
        }
    }
}

pub(crate) fn chunk_at_screen_position(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    screen_position: Vec2,
    plane_y: f32,
) -> Option<Chunk> {
    let ray = camera
        .viewport_to_world(camera_transform, screen_position)
        .ok()?;
    let direction = ray.direction.as_vec3();
    if direction.y.abs() < f32::EPSILON {
        return None;
    }

    let distance = (plane_y - ray.origin.y) / direction.y;
    if distance <= 0.0 {
        return None;
    }

    Some(Chunk::from(ray.origin + direction * distance))
}

fn spawn_debug_overlay(mut commands: Commands) {
    commands.spawn((
        DebugOverlay,
        Text::new("Chunk: --\nF4 grid | F5 labels | F7 report | F8 pin | F9 clear pins"),
        TextFont::from_font_size(12.0),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(8.0),
            bottom: Val::Px(8.0),
            padding: UiRect::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
    ));
}

#[allow(clippy::too_many_arguments)]
fn update_debug_overlay(
    time: Res<Time>,
    config: Res<DebugViewConfig>,
    hovered: Res<HoveredChunk>,
    chunk_map: Res<ChunkMap>,
    q_debug: Query<&ChunkDebug>,
    pins: Query<&DebugPin>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut overlay: Query<&mut Text, With<DebugOverlay>>,
    mut elapsed: Local<f32>,
) {
    *elapsed += time.delta_secs();
    if *elapsed < OVERLAY_REFRESH_SECS {
        return;
    }
    *elapsed = 0.0;

    let holes = q_debug
        .iter()
        .filter(|debug| debug.state == crate::ChunkDebugState::Empty)
        .count();

    let chunk_line = hovered
        .entity
        .and_then(|entity| q_debug.get(entity).ok())
        .map(|debug| {
            format!(
                "Hover ({}, {}) #{} {:?} verts={} faces={} hole={}",
                debug.x,
                debug.z,
                debug.id,
                debug.state,
                debug.vertex_count,
                debug.face_count,
                debug.state == crate::ChunkDebugState::Failed
            )
        })
        .unwrap_or_else(|| "Hover: --".to_string());

    let camera_chunk = active_camera_transform(&cameras)
        .map(|(position, _)| Chunk::from(position))
        .map(|chunk| format!("Camera chunk: ({}, {})", chunk.x, chunk.z))
        .unwrap_or_else(|| "Camera chunk: --".to_string());

    let crosshair = cameras
        .iter()
        .find(|(camera, _)| camera.is_active)
        .and_then(|(camera, transform)| {
            let screen = camera.logical_viewport_size()? / 2.0;
            let plane_y = grid_y(&config, transform.translation());
            chunk_at_screen_position(camera, transform, screen, plane_y)
        })
        .map(|chunk| format!("Crosshair chunk: ({}, {})", chunk.x, chunk.z))
        .unwrap_or_else(|| "Crosshair chunk: --".to_string());

    if let Ok(mut text) = overlay.single_mut() {
        text.0 = format!(
            "{chunk_line}\n{camera_chunk} | {crosshair}\nLoaded: {} | Pins: {} | Empty meshes: {holes}\nF4 grid[{}] | F5 labels[{}] | F7 report | F8 pin | F9 clear pins",
            chunk_map.len(),
            pins.iter().count(),
            config.show_grid,
            config.show_labels,
        );
    }
}

#[derive(Serialize)]
struct DebugReport {
    created_at_unix_ms: u128,
    version: &'static str,
    camera: Option<CameraReport>,
    hovered: Option<ChunkDebug>,
    crosshair: Option<ChunkDebug>,
    client_memory: MemoryReport,
    server_memory: Option<MemoryReport>,
    pins: Vec<PinReport>,
    nearby_chunks: Vec<ChunkDebug>,
}

#[derive(Serialize)]
struct CameraReport {
    position: [f32; 3],
    forward: [f32; 3],
}

#[derive(Serialize)]
struct MemoryReport {
    rss_bytes: u64,
    peak_rss_bytes: u64,
    age_secs: Option<f32>,
}

#[derive(Serialize)]
struct PinReport {
    id: u64,
    x: i32,
    z: i32,
    position: [f32; 3],
    label: String,
}

#[allow(clippy::too_many_arguments)]
fn capture_report(
    mut request: ResMut<ReportRequest>,
    hovered: Res<HoveredChunk>,
    client_memory: Res<ProcessMemory>,
    server_memory: Res<ServerMemory>,
    q_debug: Query<&ChunkDebug>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    config: Res<DebugViewConfig>,
    pins: Query<&DebugPin>,
    mut commands: Commands,
) {
    if !request.0 {
        return;
    }
    request.0 = false;

    let Some((camera, camera_transform)) = cameras.iter().find(|(camera, _)| camera.is_active)
    else {
        warn!("cannot capture debug report: no active camera");
        return;
    };
    let camera_position = camera_transform.translation();
    let camera_forward = camera_transform.forward().as_vec3();

    let root = std::env::var("BEVY_ASSET_ROOT").unwrap_or_else(|_| ".".to_string());
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = PathBuf::from(root)
        .join("debug_reports")
        .join(timestamp.to_string());

    if let Err(error) = fs::create_dir_all(&dir) {
        error!("failed to create {}: {error}", dir.display());
        return;
    }

    let camera_chunk = Chunk::from(camera_position);
    let nearby_chunks = q_debug
        .iter()
        .filter(|debug| {
            (debug.x - camera_chunk.x).abs() <= 4 && (debug.z - camera_chunk.z).abs() <= 4
        })
        .cloned()
        .collect::<Vec<_>>();

    let hovered = hovered
        .entity
        .and_then(|entity| q_debug.get(entity).ok())
        .cloned();

    let crosshair = camera
        .logical_viewport_size()
        .and_then(|size| {
            chunk_at_screen_position(
                camera,
                camera_transform,
                size / 2.0,
                grid_y(&config, camera_position),
            )
        })
        .and_then(|chunk| {
            q_debug
                .iter()
                .find(|debug| debug.x == chunk.x && debug.z == chunk.z)
                .cloned()
        });

    let pins = pins
        .iter()
        .map(|pin| PinReport {
            id: pin.id,
            x: pin.x,
            z: pin.z,
            position: pin.position.to_array(),
            label: pin.label.clone(),
        })
        .collect();

    let report = DebugReport {
        created_at_unix_ms: timestamp,
        version: env!("CARGO_PKG_VERSION"),
        camera: Some(CameraReport {
            position: camera_position.to_array(),
            forward: camera_forward.to_array(),
        }),
        hovered,
        crosshair,
        client_memory: MemoryReport {
            rss_bytes: client_memory.rss_bytes,
            peak_rss_bytes: client_memory.peak_rss_bytes,
            age_secs: None,
        },
        server_memory: server_memory.last_report.map(|report| MemoryReport {
            rss_bytes: server_memory.rss_bytes,
            peak_rss_bytes: server_memory.peak_rss_bytes,
            age_secs: Some(report.elapsed().as_secs_f32()),
        }),
        pins,
        nearby_chunks,
    };

    let report_path = dir.join("report.json");
    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            if let Err(error) = fs::write(&report_path, json) {
                error!("failed to write {}: {error}", report_path.display());
            }
        }
        Err(error) => error!("failed to serialize debug report: {error}"),
    }

    let readme = "Projekto debug report\n\n\
                  screenshot.png: view at capture time\n\
                  report.json: camera, hovered chunk, nearby chunks, pins and memory\n\n\
                  Inspect the archive from WSL:\n\
                  just archive holes archive/region\n\
                  just archive dump archive/region <chunk-x> <chunk-z> --map\n";
    let _ = fs::write(dir.join("README.txt"), readme);

    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(dir.join("screenshot.png")));

    info!("debug report written to {}", dir.display());
}
