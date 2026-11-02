use bevy::{
    prelude::*,
    remote::{BrpError, BrpResult, RemotePlugin, http::RemoteHttpPlugin},
};
use projekto_core::coords::Chunk;
use projekto_debug::{ChunkDebug, DebugPin, DebugViewConfig};
use projekto_diagnostics::ProcessMemory;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    chunks::{ReportRequest, chunk_at_screen_position, grid_y, spawn_pin},
    memory::ServerMemory,
};

use crate::PlayerLandscape;
#[cfg(feature = "ai")]
use crate::ai::AiSnapshot;

pub(crate) struct RemoteDebugPlugin;

const CLIENT_BRP_PORT: u16 = if cfg!(feature = "ai") { 15722 } else { 15712 };

impl Plugin for RemoteDebugPlugin {
    fn build(&self, app: &mut App) {
        let remote = RemotePlugin::default()
            .with_method_main("projekto/chunk_info", chunk_info)
            .with_method_main("projekto/pins", pins)
            .with_method_main("projekto/pin", pin)
            .with_method_main("projekto/unpin", unpin)
            .with_method_main("projekto/memory", memory)
            .with_method_main("projekto/player_landscape", player_landscape)
            .with_method_main("projekto/chunk_at_screen", chunk_at_screen)
            .with_method_main("projekto/set_camera", set_camera)
            .with_method_main("projekto/report", report);

        #[cfg(feature = "ai")]
        let remote = remote.with_method_main("projekto/ai_snapshot", ai_snapshot);

        app.add_plugins((
            remote,
            RemoteHttpPlugin::default().with_port(CLIENT_BRP_PORT),
        ));
    }
}

#[derive(Deserialize)]
struct ChunkParams {
    x: i32,
    z: i32,
}

#[derive(Deserialize)]
struct PinParams {
    x: i32,
    z: i32,
    position: Option<[f32; 3]>,
}

#[derive(Deserialize)]
struct ScreenChunkParams {
    x: f32,
    y: f32,
    plane_y: Option<f32>,
}

#[derive(Deserialize)]
struct CameraParams {
    position: [f32; 3],
    target: [f32; 3],
}

fn parse<T: serde::de::DeserializeOwned>(params: Option<Value>) -> Result<T, BrpError> {
    serde_json::from_value(params.unwrap_or(Value::Null)).map_err(|error| BrpError {
        code: -32602,
        message: format!("invalid params: {error}"),
        data: None,
    })
}

fn json<T: serde::Serialize>(value: T) -> BrpResult {
    serde_json::to_value(value).map_err(|error| BrpError {
        code: -32603,
        message: format!("failed to serialize response: {error}"),
        data: None,
    })
}

fn chunk_info(In(params): In<Option<Value>>, q: Query<&ChunkDebug>) -> BrpResult {
    let params: ChunkParams = parse(params)?;
    let debug = q
        .iter()
        .find(|debug| debug.x == params.x && debug.z == params.z)
        .cloned()
        .ok_or_else(|| BrpError {
            code: -32001,
            message: format!("chunk ({}, {}) is not loaded", params.x, params.z),
            data: None,
        })?;

    json(debug)
}

fn pins(_: In<Option<Value>>, q: Query<&DebugPin>) -> BrpResult {
    json(q.iter().collect::<Vec<_>>())
}

fn pin(In(params): In<Option<Value>>, mut commands: Commands) -> BrpResult {
    let params: PinParams = parse(params)?;
    let chunk = Chunk::new(params.x, params.z);
    let position = params.position.map(Vec3::from).unwrap_or(Vec3::new(
        chunk.x as f32 * Chunk::X_AXIS_SIZE as f32 + Chunk::X_AXIS_SIZE as f32 / 2.0,
        256.0,
        chunk.z as f32 * Chunk::Z_AXIS_SIZE as f32 + Chunk::Z_AXIS_SIZE as f32 / 2.0,
    ));

    spawn_pin(&mut commands, chunk, position);

    Ok(json!({ "ok": true, "x": chunk.x, "z": chunk.z }))
}

fn unpin(
    In(params): In<Option<Value>>,
    q: Query<(Entity, &DebugPin)>,
    mut commands: Commands,
) -> BrpResult {
    let params: ChunkParams = parse(params)?;
    let mut removed = 0;

    for (entity, pin) in &q {
        if pin.x == params.x && pin.z == params.z {
            commands.entity(entity).despawn();
            removed += 1;
        }
    }

    if removed == 0 {
        return Err(BrpError {
            code: -32001,
            message: format!("no pin at chunk ({}, {})", params.x, params.z),
            data: None,
        });
    }

    Ok(json!({ "ok": true, "removed": removed }))
}

fn memory(
    _: In<Option<Value>>,
    client: Res<ProcessMemory>,
    server: Res<ServerMemory>,
) -> BrpResult {
    Ok(json!({
        "client": {
            "rss_bytes": client.rss_bytes,
            "peak_rss_bytes": client.peak_rss_bytes,
        },
        "server": {
            "rss_bytes": server.rss_bytes,
            "peak_rss_bytes": server.peak_rss_bytes,
            "age_secs": server.last_report.map(|report| report.elapsed().as_secs_f32()),
        },
    }))
}

fn player_landscape(_: In<Option<Value>>, landscape: Option<Res<PlayerLandscape>>) -> BrpResult {
    json(landscape.as_deref().copied())
}

fn chunk_at_screen(
    In(params): In<Option<Value>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    config: Res<DebugViewConfig>,
) -> BrpResult {
    let params: ScreenChunkParams = parse(params)?;
    let Some((camera, transform)) = cameras.iter().find(|(camera, _)| camera.is_active) else {
        return Err(BrpError {
            code: -32001,
            message: "no active Camera3d".to_string(),
            data: None,
        });
    };

    let plane_y = params
        .plane_y
        .unwrap_or_else(|| grid_y(&config, transform.translation()));
    let Some(chunk) =
        chunk_at_screen_position(camera, transform, Vec2::new(params.x, params.y), plane_y)
    else {
        return Err(BrpError {
            code: -32602,
            message: "screen position does not intersect the debug grid plane".to_string(),
            data: None,
        });
    };

    Ok(json!({ "x": chunk.x, "z": chunk.z, "plane_y": plane_y }))
}

fn set_camera(
    In(params): In<Option<Value>>,
    mut cameras: Query<(&mut Transform, &Camera), With<Camera3d>>,
) -> BrpResult {
    let params: CameraParams = parse(params)?;

    for (mut transform, camera) in &mut cameras {
        if camera.is_active {
            *transform = Transform::from_translation(Vec3::from(params.position))
                .looking_at(Vec3::from(params.target), Vec3::Y);
            return Ok(json!({ "ok": true, "position": params.position, "target": params.target }));
        }
    }

    Err(BrpError {
        code: -32001,
        message: "no active Camera3d".to_string(),
        data: None,
    })
}

fn report(_: In<Option<Value>>, mut request: ResMut<ReportRequest>) -> BrpResult {
    request.0 = true;
    Ok(json!({ "ok": true }))
}

#[cfg(feature = "ai")]
fn ai_snapshot(_: In<Option<Value>>, mut request: ResMut<AiSnapshot>) -> BrpResult {
    request.0 = true;
    Ok(json!({ "ok": true }))
}
