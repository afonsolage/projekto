use criterion::{Criterion, criterion_group, criterion_main};
use projekto_core::{chunk::ChunkStorage, voxel::FacesSoftLight};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("zip 2", |b| {
        let s1 = ChunkStorage::<u16>::default();
        let s2 = ChunkStorage::<FacesSoftLight>::default();

        b.iter(|| {
            std::hint::black_box(s1.zip(&s2));
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
