use criterion::{Criterion, criterion_group, criterion_main};
use projekto_core::chunk::{self, ChunkStorage};
use projekto_server::archive::Archive;
use rand::{Rng, SeedableRng, rngs::StdRng};

fn generate_chunk(seed: u64) -> ChunkStorage<u128> {
    let mut rnd = StdRng::seed_from_u64(seed);
    let mut chunk = ChunkStorage::<u128>::default();
    chunk::voxels().for_each(|voxel| {
        chunk.set(voxel, rnd.random());
    });

    chunk
}

fn fill_archive(archive: &mut Archive<ChunkStorage<u128>>) {
    for x in 0..15u8 {
        for z in 0..15u8 {
            let chunk = generate_chunk((x as u64) << 16 | z as u64);
            archive.write(x, z, &chunk).unwrap();
        }
    }
}

fn archive_bench(c: &mut Criterion) {
    let temp_dir = std::env::temp_dir();
    let now = std::time::Instant::now().elapsed().as_micros();
    let path = format!("{}/archive_bench_{now}.tmp", temp_dir.display());

    let mut archive = Archive::new(&path).unwrap();
    fill_archive(&mut archive);
    archive.save_header().unwrap();

    c.bench_function("archive read", |b| {
        b.iter(|| {
            std::hint::black_box(archive.read(3, 3).unwrap());
        });
    });
}

criterion_group!(benches, archive_bench);
criterion_main!(benches);
