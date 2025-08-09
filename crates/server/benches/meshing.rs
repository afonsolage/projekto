use criterion::{criterion_group, criterion_main, Criterion};
use projekto_server::{
    meshing::{generate_faces, generate_vertices},
    ChunkAsset,
};

pub fn criterion_benchmark(c: &mut Criterion) {
    let ChunkAsset {
        kind,
        occlusion,
        soft_light,
        ..
    } = setup();

    println!("Kind: {kind:?}");
    println!("Occlusion: {occlusion:?}");
    println!("Soft Light: {soft_light:?}");

    let faces = generate_faces(&kind, &occlusion, &soft_light);
    let vertices = generate_vertices(&faces);

    println!("Faces: {:?}", faces.len());
    println!("Vertices: {:?}", vertices.len());

    c.bench_function("generate faces", |b| {
        b.iter(|| {
            std::hint::black_box(generate_faces(&kind, &occlusion, &soft_light));
        });
    });

    c.bench_function("generate vertices", |b| {
        b.iter(|| {
            std::hint::black_box(generate_vertices(&faces));
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
