use bevy::math::IVec3;
use criterion::{Criterion, criterion_group, criterion_main};
use projekto_core::{
    chunk::{self, ChunkStorage},
    voxel,
};
use projekto_server::{
    ChunkAsset,
    meshing::{faces_occlusion, generate_vertices, greedy},
};

pub fn criterion_benchmark(c: &mut Criterion) {
    let ChunkAsset {
        kind,
        light,
        occlusion,
        soft_light,
        chunk,
        ..
    } = setup();

    println!("Kind: {kind:?}");
    println!("Light: {light:?}");
    println!("Occlusion: {occlusion:?}");
    println!("Soft Light: {soft_light:?}");

    let greedy = greedy::generate_faces(&kind, &occlusion, &soft_light);
    let vertices_greedy = generate_vertices(&greedy);

    println!("Greedy: {:?}", greedy.len());
    println!("Vertices Greedy: {:?}", vertices_greedy.len());

    c.bench_function("generate greedy faces", |b| {
        b.iter(|| {
            std::hint::black_box(greedy::generate_faces(&kind, &occlusion, &soft_light));
        });
    });

    c.bench_function("generate vertices", |b| {
        b.iter(|| {
            std::hint::black_box(generate_vertices(&greedy));
        });
    });

    let mut occlusion = ChunkStorage::default();
    let neighborhood = [Some(&kind); chunk::SIDE_COUNT];

    c.bench_function("faces occlusion", |b| {
        b.iter(|| {
            faces_occlusion(&kind, &mut occlusion, &neighborhood);
        });
    });

    c.bench_function("gather neighborhood light", |b| {
        b.iter(|| {
            std::hint::black_box(projekto_server::light::gather_neighborhood_light(
                chunk,
                IVec3::ZERO,
                |_| Some(&kind),
                |_| Some(&light),
            ));
        });
    });

    let mut soft_light = Default::default();
    c.bench_function("faces light softening", |b| {
        b.iter(|| {
            projekto_server::light::smooth_lighting(
                chunk,
                &occlusion,
                &mut soft_light,
                |_| Some(&kind),
                |_| Some(&light),
            );
        });
    });
}

fn setup() -> ChunkAsset {
    let path = std::path::Path::new("../../chunks/0_0");
    let bytes = std::fs::read(path).unwrap();
    let (asset, _) =
        bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    asset
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
