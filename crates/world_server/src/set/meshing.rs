use bevy::{prelude::*, utils::HashSet};
use projekto_core::{chunk, voxel};

use crate::{light, meshing, WorldSet};

use crate::bundle::{
    ChunkFacesOcclusion, ChunkFacesSoftLight, ChunkKind, ChunkLight, ChunkLocal, ChunkQuery,
    ChunkVertex,
};

pub struct MeshingPlugin;

impl Plugin for MeshingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                faces_occlusion, //.run_if(any_chunk::<Changed<ChunkKind>>),
                faces_light_softening,
                // .run_if(any_chunk::<Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>),
                generate_vertices,
                // .run_if(any_chunk::<Or<(Changed<ChunkKind>, Changed<ChunkLight>)>>),
            )
                .chain()
                .in_set(WorldSet::Meshing),
        );
    }
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
            let (_, _, _, occlusion) = q_chunks.get_chunk(chunk).expect("Chunk must exists");

            let mut soft_light = q_soft_light
                .get_chunk_mut(chunk)
                .expect("Chunk must exists");

            light::smooth_lighting(
                chunk,
                occlusion,
                &mut soft_light,
                |chunk| q_chunks.get_chunk(chunk).map(|c| &**c.1),
                |chunk| q_chunks.get_chunk(chunk).map(|c| &**c.2),
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
