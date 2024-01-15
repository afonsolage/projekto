use bevy_log::{trace, warn};
use bevy_math::{IVec3, Vec3};
use bevy_tasks::AsyncComputeTaskPool;
use bevy_utils::HashSet;
use bracket_noise::prelude::{FastNoise, FractalType, NoiseType};
use itertools::Itertools;

use light_smoother::ChunkSmoothLight;
use projekto_core::{
    chunk::{ChunkKind, ChunkLight},
    voxel::{self, ChunkFacesOcclusion, FacesOcclusion},
};

use projekto_core::{
    chunk::{self, Chunk, ChunkNeighborhood},
    voxel::{VoxelFace, VoxelVertex},
    VoxWorld,
};

// mod faces_merger;
mod light_propagator;
mod light_smoother;

// v3               v2
// +-----------+
// v7  / |      v6 / |
// +-----------+   |
// |   |       |   |
// |   +-------|---+
// | /  v0     | /  v1
// +-----------+
// v4           v5
//
// Y
// |
// +---X
// /
// Z

pub const VERTICES: [[f32; 3]; 8] = [
    [0.0, 0.0, 0.0], // v0
    [1.0, 0.0, 0.0], // v1
    [1.0, 1.0, 0.0], // v2
    [0.0, 1.0, 0.0], // v3
    [0.0, 0.0, 1.0], // v4
    [1.0, 0.0, 1.0], // v5
    [1.0, 1.0, 1.0], // v6
    [0.0, 1.0, 1.0], // v7
];

pub const VERTICES_INDICES: [[usize; 4]; 6] = [
    [5, 1, 2, 6], // RIGHT
    [0, 4, 7, 3], // LEFT
    [7, 6, 2, 3], // UP
    [0, 1, 5, 4], // DOWN
    [4, 5, 6, 7], // FRONT
    [1, 0, 3, 2], // BACK
];

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
    // Each 4 vertex is a voxel face and each voxel face has 6 indices, so we can multiply the
    // vertex count by 1.5
    let index_count = (vertex_count as f32 * 1.5) as usize;

    let mut res = vec![0; index_count];
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

/// Generates a new chunk filling it with [`ChunkKind`] randomly generated by seeded noise
pub fn generate_chunk(local: IVec3) -> Chunk {
    // TODO: Move this to a config per-biome
    let mut noise = FastNoise::seeded(15);
    noise.set_noise_type(NoiseType::SimplexFractal);
    noise.set_frequency(0.03);
    noise.set_fractal_type(FractalType::FBM);
    noise.set_fractal_octaves(3);
    noise.set_fractal_gain(0.9);
    noise.set_fractal_lacunarity(0.5);
    let world = chunk::to_world(local);

    let mut kinds = ChunkKind::default();
    let mut lights = ChunkLight::default();

    for x in 0..chunk::X_AXIS_SIZE {
        for z in 0..chunk::Z_AXIS_SIZE {
            lights.set(
                (x as i32, chunk::Y_END, z as i32).into(),
                voxel::Light::natural(voxel::Light::MAX_NATURAL_INTENSITY),
            );

            let h = noise.get_noise(world.x + x as f32, world.z + z as f32);
            let world_height = ((h + 1.0) / 2.0) * (chunk::X_AXIS_SIZE * 2) as f32;

            let height_local = world_height - world.y;

            if height_local < f32::EPSILON {
                continue;
            }

            let end = usize::min(height_local as usize, chunk::Y_AXIS_SIZE);

            for y in 0..end {
                // TODO: Check this in a biome settings
                let kind = voxel::Kind::get_kind_with_height_source(end - 1, y);

                kinds.set((x as i32, y as i32, z as i32).into(), kind);
            }
        }
    }

    Chunk {
        kinds,
        lights,
        ..Default::default()
    }
}

/// Build chunk internal data without using world.
/// This is need in order to increase parallelism.
///
/// This function split the natural light propagation across tasks on [`AsyncComputeTaskPool`].
/// Each task has it's own world, due to how [`light_propagator`] works.
///
/// **Returns** returns back the same list given, but with propagated data.
pub async fn build_chunk_internals(chunks: Vec<(IVec3, Chunk)>) -> Vec<(IVec3, Chunk)> {
    trace!("Building chunk internals {}", chunks.len());

    let locals = chunks.iter().map(|(local, _)| *local).collect::<Vec<_>>();

    // VoxWorld is just a map, so it's cheap to create. -- For now
    let mut world = VoxWorld::default();
    chunks
        .into_iter()
        .for_each(|(local, chunk)| world.add(local, chunk));

    update_kind_neighborhoods(&mut world, &locals);

    // TODO: Check if this worth sending to async compute task pool

    // Split the chunks into many worlds, based on number of threads available.
    // Since this propagation is internal only, doesn't uses neighbor, it's safe to split chunks in
    // many worlds.
    let parallel_tasks = AsyncComputeTaskPool::get().thread_num();
    let chunk_split = usize::clamp(locals.len() / parallel_tasks, 1, locals.len());
    let mut tasks = vec![];

    for chunks in world
        .extract()
        .into_iter()
        .chunks(chunk_split)
        .into_iter()
        .map(|c| c.collect::<Vec<_>>())
    {
        let mut inner_world = VoxWorld::default();
        chunks
            .into_iter()
            .for_each(|(local, chunk)| inner_world.add(local, chunk));

        let task = AsyncComputeTaskPool::get().spawn(async move {
            let locals = inner_world.list_chunks();
            light_propagator::propagate_natural_light_on_new_chunk(&mut inner_world, &locals);
            inner_world
        });

        tasks.push(task);
    }

    let mut world = VoxWorld::default();
    // Gather all chunks into a new world again
    for task in tasks {
        for (local, chunk) in task.await.extract() {
            world.add(local, chunk);
        }
    }

    assert_eq!(world.list_chunks().len(), locals.len());

    // Propagate light across neighborhood. All chunks needs to be on same world.
    light_propagator::propagate_light_to_neighborhood(&mut world, &locals);

    world.extract()
}

/// Applies a list of voxel kind update on the given world.
///
/// **Returns** a list of dirty chunk with has been modified and needs to regenerate vertices.
pub fn update_chunks(
    world: &mut VoxWorld,
    update: &[(IVec3, Vec<(IVec3, voxel::Kind)>)],
) -> Vec<IVec3> {
    let mut dirty = update_kind(world, update);

    dirty.extend(light_propagator::update_light(world, update));

    // TODO: Update water, stability and so one

    dirty.into_iter().unique().collect_vec()
}

/// Update neighborhood data and propagate data across neighbors.
///
/// This function should be called whenever a chunk has it's neighbor updated.
///
/// Returns a list of dirty chunks, which needs to have their vertices recomputed.
pub fn update_neighborhood(world: &mut VoxWorld, dirty: &[IVec3]) -> Vec<IVec3> {
    update_kind_neighborhoods(world, dirty);
    light_propagator::propagate_light_to_neighborhood(world, dirty)
}

/// Apply a given list of update [`voxel::Kind`] on chunks.
///
/// This function also update neighborhood to keep it in sync.
///
/// Return a list of chunks which was updated, either direct on indirect (it's neighbor has been
/// changed).
fn update_kind(world: &mut VoxWorld, update: &[(IVec3, Vec<(IVec3, voxel::Kind)>)]) -> Vec<IVec3> {
    let mut dirty = HashSet::default();

    for (local, voxels) in update {
        if let Some(chunk) = world.get_mut(*local) {
            if voxels.is_empty() {
                continue;
            }

            dirty.insert(*local);

            trace!("Updating chunk {} values {:?}", local, voxels);

            for &(voxel, kind) in voxels {
                chunk.kinds.set(voxel, kind);

                // If this updates happens at the edge of chunk, mark neighbors chunk as dirty,
                // since this will likely affect'em
                dirty.extend(chunk::neighboring(*local, voxel));
            }
        } else {
            warn!("Failed to set voxel. Chunk {} wasn't found.", local);
        }
    }

    let dirty = dirty.into_iter().filter(|l| world.exists(*l)).collect_vec();
    update_kind_neighborhoods(world, &dirty);

    dirty
}

/// Generate the final list of vertices of the given chunks.
pub fn generate_chunk_vertices(
    world: &VoxWorld,
    locals: &[IVec3],
) -> Vec<(IVec3, Vec<VoxelVertex>)> {
    trace!("Generating vertices for {} chunks", locals.len());

    let temp_data = locals
        .iter()
        .map(|&l| (l, world.get(l).unwrap()))
        .map(|(l, chunk)| (l, faces_occlusion(chunk)))
        .map(|(l, occ)| (l, light_smoother::smooth_lighting(world, l, &occ), occ))
        .collect_vec();

    trace!(
        "Faces occlusion and light smoothing completed on {} chunks",
        temp_data.len()
    );

    temp_data
        .into_iter()
        .filter_map(|(local, smooth_light, occlusion)| {
            if occlusion.is_fully_occluded() {
                Some((local, vec![]))
            } else {
                let faces = generate_faces(occlusion, smooth_light, world.get(local)?);
                Some((local, generate_vertices(faces)))
            }
        })
        .collect()
}

/// Computes the faces occlusion data of the given [`ChunkKind`]
///
/// Returns** computed [`ChunkFacesOcclusion`]
fn faces_occlusion(chunk: &Chunk) -> ChunkFacesOcclusion {
    let kinds = &chunk.kinds;

    let mut occlusion = ChunkFacesOcclusion::default();
    for voxel in chunk::voxels() {
        let mut voxel_faces = FacesOcclusion::default();

        if kinds.get(voxel).is_none() {
            voxel_faces.set_all(true);
        } else {
            for side in voxel::SIDES {
                let dir = side.dir();
                let neighbor_pos = voxel + dir;

                if let Some(neighbor_kind) = kinds.get_absolute(neighbor_pos) {
                    voxel_faces.set(side, !neighbor_kind.is_none());
                }
            }
        }

        occlusion.set(voxel, voxel_faces);
    }

    occlusion
}

fn generate_faces(
    occlusion: ChunkFacesOcclusion,
    smooth_light: ChunkSmoothLight,
    chunk: &Chunk,
) -> Vec<VoxelFace> {
    let mut faces_vertices = vec![];

    for voxel in chunk::voxels() {
        for side in voxel::SIDES {
            // Since this is a top-down game, we don't need down face at all
            if side == voxel::Side::Down {
                continue;
            }

            let kind = chunk.kinds.get(voxel);

            if kind.is_none() || (occlusion.get(voxel).is_occluded(side)) {
                continue;
            }

            let smooth_light = smooth_light.get(voxel);

            let (v1, v2, v3, v4) = (voxel, voxel, voxel, voxel);
            faces_vertices.push(VoxelFace {
                vertices: [v1, v2, v3, v4],
                side,
                kind,
                light: smooth_light.get(side),
                voxel: [
                    projekto_core::math::pack(v1.x as u8, v1.y as u8, v1.z as u8, 0),
                    projekto_core::math::pack(v2.x as u8, v2.y as u8, v2.z as u8, 0),
                    projekto_core::math::pack(v3.x as u8, v3.y as u8, v3.z as u8, 0),
                    projekto_core::math::pack(v4.x as u8, v4.y as u8, v4.z as u8, 0),
                ],
            });
        }
    }

    faces_vertices
}

/// Generates vertices data from a given [`VoxelFace`] list.
///
/// All generated indices will be relative to a triangle list.
///
/// Returns** a list of generated [`VoxelVertex`].
fn generate_vertices(faces: Vec<VoxelFace>) -> Vec<VoxelVertex> {
    let mut vertices = vec![];
    let kinds_descs = voxel::KindsDescs::get();
    let tile_texture_size = (kinds_descs.count_tiles() as f32).recip();

    for face in faces {
        let normal = face.side.normal();

        let face_desc = kinds_descs.get_face_desc(&face);
        let tile_coord_start = face_desc.offset.as_vec2() * tile_texture_size;

        let faces_vertices = face
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let base_vertex_idx = VERTICES_INDICES[face.side as usize][i];
                let base_vertex: Vec3 = VERTICES[base_vertex_idx].into();

                base_vertex + v.as_vec3()
            })
            .collect::<Vec<_>>();

        debug_assert!(
            faces_vertices.len() == 4,
            "Each face should have 4 vertices"
        );

        fn calc_tile_size(min: Vec3, max: Vec3) -> f32 {
            (min.x - max.x).abs() + (min.y - max.y).abs() + (min.z - max.z).abs()
        }

        let x_tile = calc_tile_size(faces_vertices[0], faces_vertices[1]) * tile_texture_size;
        let y_tile = calc_tile_size(faces_vertices[0], faces_vertices[3]) * tile_texture_size;

        let tile_uv = [
            (0.0, y_tile).into(),
            (x_tile, y_tile).into(),
            (x_tile, 0.0).into(),
            (0.0, 0.0).into(),
        ];

        let light_fraction = (voxel::Light::MAX_NATURAL_INTENSITY as f32).recip();

        for (i, v) in faces_vertices.into_iter().enumerate() {
            vertices.push(VoxelVertex {
                position: v,
                normal,
                uv: tile_uv[i],
                tile_coord_start,
                light: Vec3::splat(face.light[i] * light_fraction),
                voxel: face.voxel[i],
            });
        }
    }

    debug_assert!(!vertices.is_empty());
    vertices
}

/// Updates the [`ChunkNeighborhood`] of a given locals given.
/// This function assumes all given chunks exists into the world and updates any neighborhood data
/// needed by chunk.
///
/// **Panics** if a given chunk local doesn't exists
fn update_kind_neighborhoods(world: &mut VoxWorld, locals: &[IVec3]) {
    for &local in locals {
        let mut neighborhood = ChunkNeighborhood::default();
        for side in voxel::SIDES {
            let dir = side.dir();
            let neighbor = local + dir;

            if let Some(neighbor_chunk) = world.get(neighbor) {
                neighborhood.set(side, &neighbor_chunk.kinds);
            }
        }

        let chunk = world.get_mut(local).unwrap();
        chunk.kinds.neighborhood = neighborhood;
    }
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;
    use projekto_core::voxel::{Light, LightTy};

    use super::*;

    fn top_voxels() -> impl Iterator<Item = IVec3> {
        (0..=chunk::X_END)
            .flat_map(|x| (0..=chunk::Z_END).map(move |z| (x, chunk::Y_END, z).into()))
    }

    fn set_natural_light_on_top_voxels(chunk: &mut Chunk) {
        let light = Light::natural(Light::MAX_NATURAL_INTENSITY);

        for local in top_voxels() {
            chunk.lights.set(local, light);
        }
    }

    fn create_test_world() -> VoxWorld {
        AsyncComputeTaskPool::get_or_init(Default::default);
        // Chunk               Neighbor
        // +----+----+        +----+----+----+
        // 11 | -- | 15 |        | -- | -- | 15 |
        // +----+----+        +----+----+----+
        // 10 | -- | -- |        | -- | -- | 15 |
        // +----+----+        +----+----+----+
        // 9  | -- | -- |        | 0  | -- | 15 |
        // +----+----+        +----+----+----+
        // 8  | -- | 2  |        | 1  | -- | 15 |
        // +----+----+        +----+----+----+
        // 7  | -- | 3  |        | -- | -- | 15 |
        // +----+----+        +----+----+----+
        // 6  | -- | 4  |        | 5  | -- | 15 |
        // +----+----+        +----+----+----+
        // 5  | -- | -- |        | 6  | -- | 15 |
        // +----+----+        +----+----+----+
        // 4  | -- | 8  |        | 7  | -- | 15 |
        // +----+----+        +----+----+----+
        // 3  | -- | 9  |        | -- | -- | 15 |
        // +----+----+        +----+----+----+
        // Y            2  | -- | 10 |        | 11 | -- | 15 |
        // |               +----+----+        +----+----+----+
        // |            1  | -- | -- |        | 12 | -- | 15 |
        // + ---- X        +----+----+        +----+----+----+
        // 0  | -- | 12 |        | 13 | 14 | 15 |
        // +----+----+        +----+----+----+
        //
        // + 14   15            0    1    2

        let mut chunk = Chunk::default();
        chunk.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (11..=chunk::Y_END).rev() {
            chunk.kinds.set((15, y, 0).into(), 0.into());
        }

        let mut neighbor = Chunk::default();
        neighbor.kinds.set_all(1.into()); // Make solid

        // Make holes to light propagate through
        for y in (0..=chunk::Y_END).rev() {
            neighbor.kinds.set((2, y, 0).into(), 0.into());
        }

        chunk.kinds.set((15, 11, 0).into(), 0.into());
        chunk.kinds.set((15, 8, 0).into(), 0.into());
        chunk.kinds.set((15, 7, 0).into(), 0.into());
        chunk.kinds.set((15, 6, 0).into(), 0.into());
        chunk.kinds.set((15, 4, 0).into(), 0.into());
        chunk.kinds.set((15, 3, 0).into(), 0.into());
        chunk.kinds.set((15, 2, 0).into(), 0.into());
        chunk.kinds.set((15, 0, 0).into(), 0.into());

        neighbor.kinds.set((0, 8, 0).into(), 0.into());
        neighbor.kinds.set((0, 9, 0).into(), 0.into());
        neighbor.kinds.set((0, 6, 0).into(), 0.into());
        neighbor.kinds.set((0, 5, 0).into(), 0.into());
        neighbor.kinds.set((0, 4, 0).into(), 0.into());
        neighbor.kinds.set((0, 2, 0).into(), 0.into());
        neighbor.kinds.set((0, 1, 0).into(), 0.into());
        neighbor.kinds.set((0, 0, 0).into(), 0.into());
        neighbor.kinds.set((1, 0, 0).into(), 0.into());
        neighbor.kinds.set((2, 0, 0).into(), 0.into());

        set_natural_light_on_top_voxels(&mut neighbor);
        set_natural_light_on_top_voxels(&mut chunk);

        // world.add((0, 0, 0).into(), chunk);
        // world.add((1, 0, 0).into(), neighbor);

        let chunks = block_on(super::build_chunk_internals(vec![
            ((0, 0, 0).into(), chunk),
            ((1, 0, 0).into(), neighbor),
        ]));

        let world = chunks
            .into_iter()
            .fold(VoxWorld::default(), |mut world, (local, chunk)| {
                world.add(local, chunk);
                world
            });

        let chunk = world.get((0, 0, 0).into()).unwrap();
        let neighbor = world.get((1, 0, 0).into()).unwrap();

        assert_eq!(
            neighbor
                .lights
                .get((0, 0, 0).into())
                .get_greater_intensity(),
            13
        );

        assert_eq!(
            chunk.lights.get((15, 0, 0).into()).get_greater_intensity(),
            12
        );

        assert_eq!(chunk.lights.get((15, 6, 0).into()).get(LightTy::Natural), 4, "Failed to compute chunk internals. This is likely a bug handled by others tests. Ignore this and fix others.");
        assert_eq!(neighbor.lights.get((0, 6, 0).into()).get(LightTy::Natural), 5, "Failed to compute chunk internals. This is likely a bug handled by others tests. Ignore this and fix others.");

        world
    }

    #[test]
    fn generate_chunk() {
        let local = (5432, 0, 5555).into();
        let chunk = super::generate_chunk(local);

        assert!(
            !chunk.kinds.is_default(),
            "Generate chunk should should not be default"
        );
    }

    #[test]
    fn update_chunks_neighbor_side_light() {
        let mut world = create_test_world();

        let update_list = [((0, 0, 0).into(), vec![((15, 10, 0).into(), 0.into())])];

        let updated = super::update_chunks(&mut world, &update_list);

        assert_eq!(
            updated.len(),
            2,
            "A voxel was updated on the chunk edge, so there should be 2 updated chunks."
        );

        let chunk = world.get((0, 0, 0).into()).unwrap();

        assert_eq!(
            chunk.kinds.get((15, 10, 0).into()),
            0.into(),
            "Voxel should be updated to new kind"
        );

        assert_eq!(
            chunk.lights.get((15, 10, 0).into()).get(LightTy::Natural),
            Light::MAX_NATURAL_INTENSITY,
            "Voxel should have a natural light propagated to it"
        );

        // let neighbor = world.get((1, 0, 0).into()).unwrap();

        // // Get the vertices facing the updated voxel on the neighbor
        // let updated_voxel_side_vertex = neighbor
        //     .vertices
        //     .iter()
        //     .find(|&v| v.normal == -Vec3::X && v.position == (0.0, 10.0, 0.0).into());

        // assert!(
        //     updated_voxel_side_vertex.is_some(),
        //     "There should be a vertex for left side on updated voxel"
        // );

        // let updated_voxel_side_vertex = updated_voxel_side_vertex.unwrap();
        // assert_eq!(
        //     updated_voxel_side_vertex.light,
        //     Vec3::new(0.25, 0.25, 0.25),
        //     "Should return 1/4 or light intensity, since all neighbors are occluded"
        // );
    }

    #[test]
    fn update_chunks_simple() {
        let mut world = VoxWorld::default();
        let local = (0, 0, 0).into();
        world.add(local, Default::default());

        let voxels = vec![
            ((0, 0, 0).into(), 1.into()),
            ((1, 1, 1).into(), 2.into()),
            ((0, chunk::Y_END, 5).into(), 3.into()),
        ];

        let dirty_chunks = super::update_chunks(&mut world, &[(local, voxels)]);

        let kinds = &world.get(local).unwrap().kinds;

        assert_eq!(kinds.get((0, 0, 0).into()), 1.into());
        assert_eq!(kinds.get((1, 1, 1).into()), 2.into());
        assert_eq!(kinds.get((0, chunk::Y_END, 5).into()), 3.into());

        assert_eq!(dirty_chunks.len(), 1, "Should have 1 dirty chunks",);
    }

    #[test]
    fn faces_occlusion_occlude_empty_chunk() {
        // Arrange
        let chunk = Chunk::default();

        // Act
        let occlusions = super::faces_occlusion(&chunk);

        // Assert
        assert!(
            occlusions.iter().all(|a| a.is_fully_occluded()),
            "A chunk full of empty-kind voxels should be fully occluded"
        );
    }

    #[test]
    fn faces_occlusion() {
        // Arrange
        let mut chunk = Chunk::default();

        // Top-Bottom occlusion
        chunk.kinds.set((1, 1, 1).into(), 1.into());
        chunk.kinds.set((1, 2, 1).into(), 1.into());

        // Full occluded voxel at (10, 10, 10)
        chunk.kinds.set((10, 10, 10).into(), 1.into());
        chunk.kinds.set((9, 10, 10).into(), 1.into());
        chunk.kinds.set((11, 10, 10).into(), 1.into());
        chunk.kinds.set((10, 9, 10).into(), 1.into());
        chunk.kinds.set((10, 11, 10).into(), 1.into());
        chunk.kinds.set((10, 10, 9).into(), 1.into());
        chunk.kinds.set((10, 10, 11).into(), 1.into());

        // Act
        let faces_occlusion = super::faces_occlusion(&chunk);

        // Assert
        let faces = faces_occlusion.get((1, 2, 1).into());

        assert_eq!(
            faces,
            [false, false, false, true, false, false].into(),
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.get((1, 1, 1).into());

        assert_eq!(
            faces,
            [false, false, true, false, false, false].into(),
            "Only down face should be occluded by the bottom voxel"
        );

        let faces = faces_occlusion.get((10, 10, 10).into());

        assert_eq!(
            faces,
            [true; voxel::SIDE_COUNT].into(),
            "Voxel fully surrounded by another non-empty voxels should be fully occluded"
        );
    }

    #[test]
    fn update_kind_neighborhoods() {
        let mut world = VoxWorld::default();

        let center = (1, 1, 1).into();
        let mut chunk = Chunk::default();
        chunk.kinds.set_all(10.into());
        world.add(center, chunk);

        for side in voxel::SIDES {
            let dir = side.dir();
            let pos = center + dir;
            let mut chunk = Chunk::default();
            chunk.kinds.set_all((side as u16).into());
            world.add(pos, chunk);
        }

        super::update_kind_neighborhoods(&mut world, &[(1, 1, 1).into()]);
        let chunk = world.get_mut(center).unwrap();

        for side in voxel::SIDES {
            match side {
                voxel::Side::Right => {
                    for a in 0..chunk::Y_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (0, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Left => {
                    for a in 0..chunk::Y_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (chunk::X_END, a as i32, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Up => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, 0, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Down => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Z_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, chunk::Y_END, b as i32).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Front => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Y_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, b as i32, 0).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
                voxel::Side::Back => {
                    for a in 0..chunk::X_AXIS_SIZE {
                        for b in 0..chunk::Y_AXIS_SIZE {
                            assert_eq!(
                                chunk
                                    .kinds
                                    .neighborhood
                                    .get(side, (a as i32, b as i32, chunk::Z_END).into()),
                                Some((side as u16).into())
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn faces_occlusion_neighborhood() {
        let mut world = VoxWorld::default();

        let mut top = Chunk::default();
        top.kinds.set_all(2.into());

        let mut down = Chunk::default();
        down.kinds.set_all(3.into());

        let mut center = Chunk::default();
        center.kinds.set((0, chunk::Y_END, 0).into(), 1.into());
        center.kinds.set((1, 0, 1).into(), 1.into());

        world.add((0, 1, 0).into(), top);
        world.add((0, 0, 0).into(), center);
        world.add((0, -1, 0).into(), down);

        super::update_kind_neighborhoods(&mut world, &[(0, 0, 0).into()]);

        let center = world.get((0, 0, 0).into()).unwrap();
        let faces_occlusion = super::faces_occlusion(center);

        let faces = faces_occlusion.get((0, chunk::Y_END, 0).into());
        assert_eq!(faces, [false, false, true, false, false, false].into());

        let faces = faces_occlusion.get((1, 0, 1).into());
        assert_eq!(faces, [false, false, false, true, false, false].into());
    }

    #[test]
    fn generate_vertices() {
        // Arrange
        let side = voxel::Side::Up;

        // This face is 2 voxels wide on the -Z axis (0,0) (0,-1)
        let faces = vec![VoxelFace {
            side,
            vertices: [
                (0, 0, 0).into(),
                (0, 0, 0).into(),
                (0, 0, -1).into(),
                (0, 0, -1).into(),
            ],
            kind: 1.into(),
            ..Default::default()
        }];

        // Act
        let vertices = super::generate_vertices(faces);

        // Assert
        let normal = side.normal();
        assert_eq!(
            vertices,
            vec![
                VoxelVertex {
                    normal,
                    position: (0.0, 1.0, 1.0).into(),
                    uv: (0.0, 0.2).into(),
                    tile_coord_start: (0.2, 0.1).into(),
                    ..Default::default()
                },
                VoxelVertex {
                    normal,
                    position: (1.0, 1.0, 1.0).into(),
                    uv: (0.1, 0.2).into(),
                    tile_coord_start: (0.2, 0.1).into(),
                    ..Default::default()
                },
                VoxelVertex {
                    normal,
                    position: (1.0, 1.0, -1.0).into(),
                    uv: (0.1, 0.0).into(),
                    tile_coord_start: (0.2, 0.1).into(),
                    ..Default::default()
                },
                VoxelVertex {
                    normal,
                    position: (0.0, 1.0, -1.0).into(),
                    uv: (0.0, 0.0).into(),
                    tile_coord_start: (0.2, 0.1).into(),
                    ..Default::default()
                },
            ]
        );
    }
}
