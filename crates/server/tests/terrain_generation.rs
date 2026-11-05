//! End-to-end checks for generated terrain and the client meshing path.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use projekto_core::{
    chunk::{self, ChunkStorage},
    coords::{Chunk, ChunkVoxel},
    voxel::{self, Kind, Light},
};
use projekto_server::{
    genesis::{ChunkCreation, GenesisServer, WorldGenConfig},
    meshing::{faces_occlusion, generate_vertices, greedy},
};

fn generate(seed: u64, chunks: &[Chunk]) -> Vec<(Chunk, ChunkCreation)> {
    let mut config = WorldGenConfig::load_default();
    config.seed = seed;
    let server = GenesisServer::with_config(2, config);

    let mut tasks: Vec<_> = chunks.iter().map(|&chunk| server.generate(chunk)).collect();
    let mut results = Vec::with_capacity(chunks.len());
    let deadline = Instant::now() + Duration::from_secs(60);

    while !tasks.is_empty() {
        tasks.retain_mut(|task| match task.try_get_result() {
            Some(result) => {
                results.push((task.chunk(), result.expect("genesis failed")));
                false
            }
            None => true,
        });

        assert!(Instant::now() < deadline, "genesis timed out");
        std::thread::sleep(Duration::from_millis(1));
    }

    results
}

fn meshing_vertices(
    chunk: Chunk,
    kind: &ChunkStorage<Kind>,
    light: &ChunkStorage<Light>,
) -> Vec<voxel::Vertex> {
    let neighborhood = [Some(kind); chunk::SIDE_COUNT];

    let mut occlusion = ChunkStorage::default();
    faces_occlusion(kind, &mut occlusion, &neighborhood);

    let mut soft_light = ChunkStorage::default();
    projekto_server::light::smooth_lighting(
        chunk,
        &occlusion,
        &mut soft_light,
        |_| Some(kind),
        |_| Some(light),
    );

    let faces = greedy::generate_faces(kind, &occlusion, &soft_light);
    generate_vertices(&faces)
}

#[test]
fn generated_terrain_has_surface_layers_and_caves() {
    let chunks = [
        Chunk::new(0, 0),
        Chunk::new(1, 0),
        Chunk::new(0, 1),
        Chunk::new(-1, -1),
    ];
    let results = generate(42, &chunks);

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut underground_air = 0usize;

    for (_chunk, creation) in &results {
        for x in 0..Chunk::X_AXIS_SIZE {
            for z in 0..Chunk::Z_AXIS_SIZE {
                let mut top = None;

                for y in (0..=Chunk::Y_END).rev() {
                    let voxel = ChunkVoxel::new(x as u8, y, z as u8);
                    let kind = creation.kind.get(voxel);

                    if kind.is_none() {
                        continue;
                    }

                    top.get_or_insert(y);
                    *counts.entry(kind.name()).or_insert(0) += 1;
                }

                if let Some(top) = top {
                    for y in 1..top.saturating_sub(8) {
                        let voxel = ChunkVoxel::new(x as u8, y, z as u8);
                        if creation.kind.get(voxel).is_none() {
                            underground_air += 1;
                        }
                    }
                }
            }
        }
    }

    assert!(
        *counts.get("Grass").unwrap_or(&0) > 0,
        "no grass: {counts:?}"
    );
    assert!(*counts.get("Dirt").unwrap_or(&0) > 0, "no dirt: {counts:?}");
    assert!(*counts.get("Rock").unwrap_or(&0) > 0, "no rock: {counts:?}");
    assert!(underground_air > 0, "no caves carved");
}

#[test]
fn generated_terrain_meshes_without_panicking() {
    let results = generate(42, &[Chunk::new(0, 0)]);

    for (chunk, creation) in &results {
        let vertices = meshing_vertices(*chunk, &creation.kind, &creation.light);

        assert!(!vertices.is_empty(), "chunk meshed to zero vertices");
        assert!(
            vertices.iter().all(|vertex| vertex.position.is_finite()
                && vertex.normal.is_finite()
                && vertex.uv.is_finite()
                && vertex.light.is_finite()),
            "mesh contains non-finite attributes"
        );
    }
}

#[test]
fn generated_terrain_is_deterministic_across_servers() {
    let a = generate(7, &[Chunk::new(3, -2)]);
    let b = generate(7, &[Chunk::new(3, -2)]);

    let (_, a) = &a[0];
    let (_, b) = &b[0];

    for voxel in chunk::voxels() {
        assert_eq!(
            a.kind.get(voxel),
            b.kind.get(voxel),
            "voxel {voxel:?} differs between runs"
        );
        assert_eq!(
            a.light.get(voxel),
            b.light.get(voxel),
            "light {voxel:?} differs between runs"
        );
    }
}
