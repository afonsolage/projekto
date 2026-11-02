mod data;

pub use data::DebugDataPlugin;

use bevy::app::{PluginGroup, PluginGroupBuilder};

/// Debug-only plugins, composed as a normal Bevy plugin group.
pub struct DebugToolsPlugins;

impl PluginGroup for DebugToolsPlugins {
    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::start::<Self>()
            .add(DebugDataPlugin)
            .add(MemoryReportPlugin);

        #[cfg(feature = "remote-debug")]
        let builder = builder.add(RemoteDebugPlugin);

        builder
    }
}

pub use memory_report::MemoryReportPlugin;

mod memory_report {
    use bevy::prelude::*;
    use projekto_diagnostics::{ProcessMemory, ProcessMemoryPlugin};
    use projekto_messages::DebugMemory;

    use crate::{WorldSet, net::Clients};

    pub struct MemoryReportPlugin;

    impl Plugin for MemoryReportPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(ProcessMemoryPlugin).add_systems(
                PostUpdate,
                send_memory_report.in_set(WorldSet::SendResponses),
            );
        }
    }

    fn send_memory_report(
        clients: Res<Clients>,
        memory: Res<ProcessMemory>,
        time: Res<Time>,
        mut timer: Local<Option<Timer>>,
    ) {
        let timer = timer.get_or_insert_with(|| Timer::from_seconds(1.0, TimerMode::Repeating));

        if !timer.tick(time.delta()).just_finished() || clients.is_empty() {
            return;
        }

        for client in clients.values() {
            let _ = client.channel().send(DebugMemory {
                rss_bytes: memory.rss_bytes,
                peak_rss_bytes: memory.peak_rss_bytes,
            });
        }
    }
}

#[cfg(feature = "remote-debug")]
pub use remote::RemoteDebugPlugin;

#[cfg(feature = "remote-debug")]
mod remote {
    use bevy::{
        math::IVec3,
        prelude::*,
        remote::{BrpError, BrpResult, RemotePlugin, http::RemoteHttpPlugin},
    };
    use projekto_core::{
        coords::{Chunk, ChunkVoxel},
        voxel::Kind,
    };
    use projekto_debug::ChunkDebug;
    use projekto_diagnostics::ProcessMemory;
    use serde::Deserialize;
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};

    use crate::{
        DerivedCaches, DirtyChunks, affected_chunks,
        bundle::{ChunkKind, ChunkMap},
        set::{Landscape, MAX_LANDSCAPE_RADIUS},
    };

    pub struct RemoteDebugPlugin;

    impl Plugin for RemoteDebugPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins((
                RemotePlugin::default()
                    .with_method_main("projekto/chunk_info", chunk_info)
                    .with_method_main("projekto/chunks", chunks)
                    .with_method_main("projekto/landscape", landscape)
                    .with_method_main("projekto/memory", memory)
                    .with_method_main("projekto/remesh", remesh)
                    .with_method_main("projekto/teleport", teleport)
                    .with_method_main("projekto/set_voxel", set_voxel),
                RemoteHttpPlugin::default(),
            ));
        }
    }

    #[derive(Deserialize)]
    struct ChunkParams {
        x: i32,
        z: i32,
    }

    #[derive(Deserialize)]
    struct AreaParams {
        x: i32,
        z: i32,
        radius: i32,
    }

    #[derive(Deserialize)]
    struct TeleportParams {
        x: i32,
        z: i32,
        radius: u8,
    }

    #[derive(Deserialize)]
    struct SetVoxelParams {
        chunk_x: i32,
        chunk_z: i32,
        x: u8,
        y: u8,
        z: u8,
        kind: u16,
    }

    fn parse<T: DeserializeOwned>(params: Option<Value>) -> Result<T, BrpError> {
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

    fn not_found(chunk: Chunk) -> BrpError {
        BrpError {
            code: -32001,
            message: format!("chunk {chunk} is not loaded"),
            data: None,
        }
    }

    fn chunk_info(
        In(params): In<Option<Value>>,
        map: Res<ChunkMap>,
        q: Query<&ChunkDebug>,
    ) -> BrpResult {
        let params: ChunkParams = parse(params)?;
        let chunk = Chunk::new(params.x, params.z);
        let entity = map.get(&chunk).ok_or_else(|| not_found(chunk))?;
        let debug = q.get(*entity).map_err(|_| not_found(chunk))?;

        json(debug)
    }

    fn chunks(In(params): In<Option<Value>>, q: Query<&ChunkDebug>) -> BrpResult {
        let params: AreaParams = parse(params)?;
        let chunks = q
            .iter()
            .filter(|debug| {
                (debug.x - params.x).abs() <= params.radius
                    && (debug.z - params.z).abs() <= params.radius
            })
            .cloned()
            .collect::<Vec<_>>();

        json(chunks)
    }

    fn landscape(_: In<Option<Value>>, landscape: Option<Res<Landscape>>) -> BrpResult {
        json(landscape.as_deref().copied())
    }

    fn memory(_: In<Option<Value>>, memory: Res<ProcessMemory>) -> BrpResult {
        json(*memory)
    }

    fn remesh(
        In(params): In<Option<Value>>,
        mut dirty: ResMut<DirtyChunks>,
        mut caches: ResMut<DerivedCaches>,
    ) -> BrpResult {
        let params: ChunkParams = parse(params)?;
        let chunk = Chunk::new(params.x, params.z);

        for affected in affected_chunks(chunk) {
            dirty.0.insert(affected);
            caches.invalidate(affected);
        }

        Ok(json!({ "ok": true, "chunk": { "x": chunk.x, "z": chunk.z } }))
    }

    fn teleport(In(params): In<Option<Value>>, mut commands: Commands) -> BrpResult {
        let params: TeleportParams = parse(params)?;
        let radius = params.radius.min(MAX_LANDSCAPE_RADIUS);

        commands.insert_resource(Landscape {
            center: IVec2::new(params.x, params.z),
            radius,
        });

        Ok(json!({ "ok": true, "center": [params.x, params.z], "radius": radius }))
    }

    fn set_voxel(
        In(params): In<Option<Value>>,
        map: Res<ChunkMap>,
        mut q_kind: Query<&mut ChunkKind>,
        mut dirty: ResMut<DirtyChunks>,
        mut caches: ResMut<DerivedCaches>,
    ) -> BrpResult {
        let params: SetVoxelParams = parse(params)?;
        let chunk = Chunk::new(params.chunk_x, params.chunk_z);
        let entity = map.get(&chunk).ok_or_else(|| not_found(chunk))?;
        let Some(voxel) = ChunkVoxel::try_from(IVec3::new(
            params.x as i32,
            params.y as i32,
            params.z as i32,
        )) else {
            return Err(BrpError {
                code: -32602,
                message: "voxel coordinates are outside the chunk".to_string(),
                data: None,
            });
        };

        let mut kind = q_kind.get_mut(*entity).map_err(|_| not_found(chunk))?;
        kind.0.set(voxel, Kind::id(params.kind));

        for affected in affected_chunks(chunk) {
            dirty.0.insert(affected);
            caches.invalidate(affected);
        }

        Ok(
            json!({ "ok": true, "chunk": { "x": chunk.x, "z": chunk.z }, "voxel": [params.x, params.y, params.z] }),
        )
    }
}
