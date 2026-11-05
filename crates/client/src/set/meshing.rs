use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use projekto_core::voxel;
use projekto_messages::{ChunkUnload, ChunkVertex};
use projekto_proto::RegisterMessageHandler;

use crate::{
    ChunkBundle, ChunkLocal, ChunkMap, ChunkMaterialHandle, ChunkMeshBuilt, ClientSet,
    material::ChunkMaterial, net::ServerDisconnected,
};

const MESH_APPLY_BUDGET: Duration = Duration::from_millis(6);
const MAX_PENDING_CHUNK_MESHES: usize = 8192;
const MAX_PENDING_CHUNK_VERTICES: usize = 2_000_000;

pub(crate) struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingChunkMeshes>()
            .set_message_handler(queue_chunk_mesh)
            .add_message_handler(remove_chunk_mesh)
            .add_systems(
                Update,
                (
                    apply_chunk_meshes.in_set(ClientSet::Meshing),
                    despawn_chunks_on_server_disconnect.run_if(on_message::<ServerDisconnected>),
                ),
            );
    }
}

#[derive(Resource, Default)]
pub(crate) struct PendingChunkMeshes {
    queue: VecDeque<ChunkVertex>,
    vertices: usize,
}

impl PendingChunkMeshes {
    fn push(&mut self, vertex: ChunkVertex) {
        if let Some(index) = self
            .queue
            .iter()
            .position(|queued| queued.chunk == vertex.chunk)
            && let Some(previous) = self.queue.remove(index)
        {
            self.vertices -= previous.vertex.len();
        }

        self.vertices += vertex.vertex.len();
        self.queue.push_back(vertex);

        while self.queue.len() > MAX_PENDING_CHUNK_MESHES
            || (self.vertices > MAX_PENDING_CHUNK_VERTICES && self.queue.len() > 1)
        {
            if let Some(previous) = self.queue.pop_front() {
                self.vertices -= previous.vertex.len();
            }
        }
    }

    fn remove_chunk(&mut self, chunk: projekto_core::coords::Chunk) {
        self.queue.retain(|queued| queued.chunk != chunk);
        self.vertices = self.queue.iter().map(|queued| queued.vertex.len()).sum();
    }

    fn clear(&mut self) {
        self.queue.clear();
        self.vertices = 0;
    }

    fn len(&self) -> usize {
        self.queue.len()
    }
}

fn queue_chunk_mesh(In(vertex): In<ChunkVertex>, mut pending: ResMut<PendingChunkMeshes>) {
    pending.push(vertex);
}

fn remove_chunk_mesh(
    In(ChunkUnload { chunk }): In<ChunkUnload>,
    mut commands: Commands,
    mut map: ResMut<ChunkMap>,
    mut pending: ResMut<PendingChunkMeshes>,
) {
    pending.remove_chunk(chunk);

    if let Some(entity) = map.remove(&chunk) {
        commands.entity(entity).despawn();
        trace!("[remove_chunk_mesh] chunk {chunk:?} despawned");
    }
}

fn despawn_chunks_on_server_disconnect(
    mut map: ResMut<ChunkMap>,
    mut pending: ResMut<PendingChunkMeshes>,
    mut reader: MessageReader<ServerDisconnected>,
    mut commands: Commands,
) {
    reader.clear();
    pending.clear();
    for (_, entity) in map.drain() {
        commands.entity(entity).despawn();
    }
}

fn apply_chunk_meshes(
    mut pending: ResMut<PendingChunkMeshes>,
    mut commands: Commands,
    mut map: ResMut<ChunkMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<ChunkMaterialHandle>,
    mut mesh_built: MessageWriter<ChunkMeshBuilt>,
) {
    if pending.len() == 0 {
        return;
    }

    let started = Instant::now();

    while let Some(vertex) = pending.queue.pop_front() {
        pending.vertices -= vertex.vertex.len();
        apply_chunk_mesh(
            vertex,
            &mut commands,
            &mut map,
            &mut meshes,
            &material,
            &mut mesh_built,
        );

        if started.elapsed() >= MESH_APPLY_BUDGET {
            break;
        }
    }
}

fn apply_chunk_mesh(
    vertex: ChunkVertex,
    commands: &mut Commands,
    map: &mut ChunkMap,
    meshes: &mut Assets<Mesh>,
    material: &ChunkMaterialHandle,
    mesh_built: &mut MessageWriter<ChunkMeshBuilt>,
) {
    let ChunkVertex { chunk, vertex } = vertex;
    let vertex_count = vertex.len() as u32;
    let top_y = vertex
        .iter()
        .map(|vertex| vertex.position.y)
        .fold(f32::MIN, f32::max);
    let center_y = center_height(vertex.iter().map(|vertex| vertex.position)).unwrap_or(top_y);

    let mesh_handler = Mesh3d(meshes.add(generate_mesh(&vertex)));

    let entity = if let Some(&entity) = map.get(&chunk) {
        commands.entity(entity).insert(mesh_handler);
        entity
    } else {
        let entity = commands
            .spawn((ChunkBundle {
                chunk: ChunkLocal(chunk),
                mesh: mesh_handler,
                material: MeshMaterial3d(material.0.clone()),
                transform: Transform::from_translation(chunk.into()),
            },))
            .insert(Name::new(format!("Client Chunk {chunk}")))
            .id();
        map.insert(chunk, entity);
        entity
    };

    mesh_built.write(ChunkMeshBuilt {
        entity,
        chunk,
        vertex_count,
        top_y: if vertex_count == 0 { 0.0 } else { top_y },
        center_y: if vertex_count == 0 { 0.0 } else { center_y },
    });

    trace!("[apply_chunk_mesh] chunk {chunk:?} mesh updated");
}

fn center_height(positions: impl Iterator<Item = Vec3>) -> Option<f32> {
    const CENTER: f32 = 8.0;
    const RADIUS: f32 = 2.0;

    positions
        .filter(|position| {
            (position.x - CENTER).abs() <= RADIUS && (position.z - CENTER).abs() <= RADIUS
        })
        .map(|position| position.y)
        .reduce(f32::max)
}

fn generate_mesh(vertices: &[voxel::Vertex]) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    let mut positions: Vec<[f32; 3]> = vec![];
    let mut normals: Vec<[f32; 3]> = vec![];
    let mut uvs: Vec<[f32; 2]> = vec![];
    let mut tile_coord_start: Vec<[f32; 2]> = vec![];
    let mut lights: Vec<[f32; 3]> = vec![];

    let vertex_count = vertices.len();

    for vertex in vertices {
        positions.push(vertex.position.into());
        normals.push(vertex.normal.into());
        uvs.push(vertex.uv.into());
        tile_coord_start.push(vertex.tile_coord_start.into());
        lights.push(vertex.light.into());
    }

    mesh.insert_indices(Indices::U32(compute_indices(vertex_count)));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(ChunkMaterial::ATTRIBUTE_TILE_COORD_START, tile_coord_start);
    mesh.insert_attribute(ChunkMaterial::ATTRIBUTE_LIGHT, lights);
    mesh
}

/// Computes indices of a triangle list mesh.
///
/// This function assumes 4 vertices per face, 3 indices per triangles and all vertices are placed
/// in CCW order.
///
/// It generates indices in the following order: _*0 1 2 2 3 0*_ where 0 is the first vertice and 3
/// is the last one
///
/// Returns** a list of indices in the CCW order
pub fn compute_indices(vertex_count: usize) -> Vec<u32> {
    // Each 4 vertices form a voxel face, which needs 6 indices.
    let mut res = Vec::with_capacity((vertex_count / 4) * 6);
    let mut i = 0u32;

    while i < vertex_count as u32 {
        res.push(i);
        res.push(i + 1);
        res.push(i + 2);

        res.push(i + 2);
        res.push(i + 3);
        res.push(i);

        i += 4;
    }

    res
}

#[cfg(test)]
mod tests {
    use bevy::app::ScheduleRunnerPlugin;
    use projekto_core::coords::Chunk;
    use projekto_proto::{ClientId, RunMessageHandlers};

    use super::*;

    #[test]
    fn compute_indices_does_not_pad() {
        assert_eq!(compute_indices(8), vec![0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4]);
    }

    #[test]
    fn chunk_unload_despawns_entity_and_removes_map_entry() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .init_resource::<ChunkMap>()
            .init_resource::<PendingChunkMeshes>();
        app.add_message_handler(remove_chunk_mesh);

        let chunk = Chunk::new(1, -2);
        let entity = app.world_mut().spawn(ChunkLocal(chunk)).id();
        app.world_mut()
            .resource_mut::<ChunkMap>()
            .insert(chunk, entity);
        app.world_mut()
            .resource_mut::<PendingChunkMeshes>()
            .push(ChunkVertex {
                chunk,
                vertex: Vec::new(),
            });

        app.world_mut()
            .run_handlers::<ChunkUnload>(ClientId::default(), Box::new(ChunkUnload { chunk }));
        app.update();

        assert!(!app.world().resource::<ChunkMap>().contains_key(&chunk));
        assert!(app.world().get_entity(entity).is_err());
        assert!(app.world().resource::<PendingChunkMeshes>().len() == 0);
    }

    #[test]
    fn queued_meshes_coalesce_by_chunk() {
        let chunk = Chunk::new(2, 3);
        let mut pending = PendingChunkMeshes::default();

        for _ in 0..3 {
            pending.push(ChunkVertex {
                chunk,
                vertex: Vec::new(),
            });
        }

        assert_eq!(pending.len(), 1, "only the newest mesh per chunk is kept");
        assert_eq!(pending.queue.front().unwrap().chunk, chunk);
    }
}
