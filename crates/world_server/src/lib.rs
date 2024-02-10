use std::time::Duration;

use bevy::{
    ecs::query::ReadOnlyWorldQuery, prelude::*, time::common_conditions::on_timer, utils::HashSet,
};
use chunk_map::ChunkQuery;
use projekto_core::{
    chunk::{self, Chunk, ChunkStorage},
    voxel::{self},
};
use set::{ChunkInitializationPlugin, ChunkManagementPlugin, LandscapePlugin, PropagationPlugin};

pub mod app;
pub mod channel;
mod genesis;
mod light;
mod meshing;

pub mod chunk_map;
pub mod set;

const MESHING_TICK_MS: u64 = 500;

pub struct WorldServerPlugin;

impl Plugin for WorldServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                WorldSet::LandscapeUpdate.before(WorldSet::ChunkManagement),
                WorldSet::ChunkManagement.before(WorldSet::FlushCommands),
                WorldSet::ChunkInitialization.after(WorldSet::FlushCommands),
                WorldSet::Propagation.after(WorldSet::ChunkInitialization),
                WorldSet::Meshing
                    .after(WorldSet::Propagation)
                    .run_if(on_timer(Duration::from_millis(MESHING_TICK_MS))),
            ),
        )
        .add_plugins((
            LandscapePlugin,
            ChunkManagementPlugin,
            ChunkInitializationPlugin,
            PropagationPlugin,
        ))
        .add_systems(
            Update,
            (
                apply_deferred.in_set(WorldSet::FlushCommands),
                // Meshing
                (
                    faces_occlusion, //.run_if(any_chunk::<Changed<ChunkKind>>),
                    faces_light_softening,
                    // .run_if(any_chunk::<Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>),
                    generate_vertices,
                    // .run_if(any_chunk::<Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>),
                )
                    .chain()
                    .in_set(WorldSet::Meshing),
            ),
        );
    }
}

#[derive(SystemSet, Debug, Copy, Clone, Hash, PartialEq, Eq)]
enum WorldSet {
    LandscapeUpdate,
    ChunkManagement,
    FlushCommands,
    ChunkInitialization,
    Propagation,
    Meshing,
}

// Components
#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkKind(ChunkStorage<voxel::Kind>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkLight(ChunkStorage<voxel::Light>);

#[derive(Component, Default, Debug, Clone, Copy, Deref, DerefMut)]
pub struct ChunkLocal(Chunk);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesOcclusion(ChunkStorage<voxel::FacesOcclusion>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
struct ChunkFacesSoftLight(ChunkStorage<voxel::FacesSoftLight>);

#[derive(Component, Default, Debug, Clone, Deref, DerefMut)]
pub struct ChunkVertex(Vec<voxel::Vertex>);

#[derive(Bundle, Default)]
struct ChunkBundle {
    kind: ChunkKind,
    light: ChunkLight,
    local: ChunkLocal,
    occlusion: ChunkFacesOcclusion,
    soft_light: ChunkFacesSoftLight,
    vertex: ChunkVertex,
}

fn any_chunk<T: ReadOnlyWorldQuery>(q_changed_chunks: Query<(), (T, With<ChunkLocal>)>) -> bool {
    !q_changed_chunks.is_empty()
}

fn faces_occlusion(
    q_changed_chunks: Query<&ChunkLocal, Changed<ChunkKind>>,
    q_kinds: ChunkQuery<&ChunkKind>,
    mut q_occlusions: ChunkQuery<&mut ChunkFacesOcclusion>,
) {
    let mut count = 0;
    let mut fully_occluded = 0;

    q_changed_chunks
        .iter()
        .flat_map(|local| {
            // TODO: There should be a better way to avoid update everything.
            // When a chunk kind is updated, we have to check all its surrounding.
            let neighbors = chunk::SIDES.map(|s| local.neighbor(s.dir()));
            std::iter::once(**local).chain(neighbors)
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|&chunk| q_kinds.chunk_exists(chunk))
        .for_each(|chunk| {
            let mut neighborhood = [None; chunk::SIDE_COUNT];

            // Update neighborhood
            chunk::SIDES.iter().for_each(|side| {
                let neighbor = chunk.neighbor(side.dir());
                neighborhood[side.index()] = q_kinds.get_chunk(neighbor).map(|kind| &**kind);
            });

            let mut faces_occlusion = q_occlusions.get_chunk_mut(chunk).expect("Entity exists");
            let kind = q_kinds.get_chunk(chunk).expect("Entity exists");
            meshing::faces_occlusion(kind, &mut faces_occlusion, &neighborhood);

            if faces_occlusion.iter().all(|occ| occ.is_fully_occluded()) {
                fully_occluded += 1;
            }
            count += 1;
        });

    if count > 0 {
        trace!("[faces_occlusion] {count} chunks faces occlusion computed. {fully_occluded} chunks fully occluded.");
    }
}

#[allow(clippy::type_complexity)]
fn faces_light_softening(
    q_changed_chunks: Query<&ChunkLocal, Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>,
    q_chunks: ChunkQuery<(&ChunkLocal, &ChunkKind, &ChunkLight, &ChunkFacesOcclusion)>,
    mut q_soft_light: ChunkQuery<&mut ChunkFacesSoftLight>,
) {
    let mut count = 0;

    q_changed_chunks
        .iter()
        .flat_map(|local| {
            // TODO: There should be a better way to avoid update everything.
            // When a chunk kind or light is updated, we have to check all its surrounding.
            let neighbors = chunk::SIDES.map(|s| local.neighbor(s.dir()));
            std::iter::once(**local).chain(neighbors)
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|&chunk| q_chunks.chunk_exists(chunk))
        .for_each(|chunk| {
            let occlusion = &**q_chunks
                .get_chunk_component::<ChunkFacesOcclusion>(chunk)
                .expect("Chunk must exists");

            let mut soft_light = q_soft_light
                .get_chunk_mut(chunk)
                .expect("Chunk must exists");

            light::smooth_lighting(
                chunk,
                occlusion,
                &mut soft_light,
                |chunk| {
                    q_chunks
                        .get_chunk_component::<ChunkKind>(chunk)
                        .map(|c| &**c)
                },
                |chunk| {
                    q_chunks
                        .get_chunk_component::<ChunkLight>(chunk)
                        .map(|c| &**c)
                },
            );

            count += 1;
        });

    if count > 0 {
        trace!("[faces_light_softening] {count} chunks faces light softened.");
    }
}

#[allow(clippy::type_complexity)]
fn generate_vertices(
    q_changed_chunks: Query<
        (
            Entity,
            &ChunkKind,
            &ChunkFacesOcclusion,
            &ChunkFacesSoftLight,
        ),
        Or<(Changed<ChunkKind>, Changed<ChunkLight>)>,
    >,
    mut q_vertex: Query<&mut ChunkVertex>,
) {
    let mut count = 0;
    let mut map = [0; voxel::SIDE_COUNT];
    q_changed_chunks
        .iter()
        .for_each(|(entity, kind, faces_occlusion, faces_soft_light)| {
            if faces_occlusion.iter().all(|occ| occ.is_fully_occluded()) {
                return;
            }

            // let faces = meshing::faces_merge(kind, faces_occlusion, faces_soft_light);
            let faces = meshing::generate_faces(kind, faces_occlusion, faces_soft_light);

            faces.iter().for_each(|face| {
                map[face.side.index()] += 1;
            });

            let mut vertex = meshing::generate_vertices(faces);

            let mut chunk_vertex = q_vertex.get_mut(entity).expect("Entity must exists");
            std::mem::swap(&mut vertex, &mut chunk_vertex);

            count += 1;
        });

    if count > 0 {
        trace!("[generate_vertices] {count} chunks vertices generated. {map:?}");
    }
}

#[cfg(test)]
mod test {
    use bevy::app::ScheduleRunnerPlugin;

    use super::*;

    #[test]
    fn plugin() {
        App::new()
            .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()))
            .add_plugins(super::WorldServerPlugin)
            .run()
    }
}

// TODO: Extract and render to check if its working.
