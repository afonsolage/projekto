use criterion::{Criterion, criterion_group, criterion_main};
use projekto_core::{chunk::ChunkStorage, coords::ChunkVoxel, voxel::FacesSoftLight};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("get", |b| {
        let s = create_storage();

        b.iter(|| {
            std::hint::black_box(s.get(ChunkVoxel::new(2, 0, 111)));
        });
    });

    c.bench_function("zip 2", |b| {
        let s1 = ChunkStorage::<u16>::default();
        let s2 = ChunkStorage::<FacesSoftLight>::default();

        b.iter(|| {
            std::hint::black_box(s1.zip(&s2));
        });
    });
}

fn create_storage() -> ChunkStorage<u128> {
    let mut storage = ChunkStorage::<u128>::default();

    for x in 1..33 {
        for z in 100..122 {
            storage.set(ChunkVoxel::new(x, 0, z), x as u128 * z as u128);
        }
    }

    storage
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
