use async_io::block_on;
use criterion::{Criterion, criterion_group, criterion_main};
use projekto_archive::Archive;
use projekto_core::{
    chunk::ChunkStorage,
    coords::{ChunkVoxel, RegionChunk},
};
use rand::{Rng, SeedableRng, rngs::StdRng};

fn generate_chunk(seed: u64) -> ChunkStorage<u128> {
    let mut rnd = StdRng::seed_from_u64(seed);
    let mut chunk = ChunkStorage::<u128>::default();
    for x in 0..15 {
        for z in 0..15 {
            for y in 0..200 {
                chunk.set(ChunkVoxel::new(x, y, z), rnd.random_range(1..80));
            }
        }
    }

    chunk
}

async fn fill_archive(archive: &mut Archive<ChunkStorage<u128>>) {
    for x in 0..15u8 {
        for z in 0..15u8 {
            let chunk = generate_chunk((x as u64) << 16 | z as u64);
            archive
                .write(RegionChunk::new(x, z), Archive::compress(&chunk).unwrap())
                .await
                .unwrap();
        }
    }
}

fn archive_bench(c: &mut Criterion) {
    let temp_dir = std::env::temp_dir();
    let now = std::time::Instant::now().elapsed().as_micros();
    let path = format!("{}/archive_bench_{now}.tmp", temp_dir.display());

    let chunk = generate_chunk(1234);

    let mut archive = block_on(Archive::new(&path)).unwrap();
    block_on(fill_archive(&mut archive));
    block_on(archive.save_header()).unwrap();

    c.bench_function("archive read", |b| {
        b.iter(|| block_on(archive.read(RegionChunk::new(3, 3))).unwrap());
    });

    let compressed = Archive::compress(&chunk).unwrap();
    c.bench_function("archive write", |b| {
        b.iter(|| block_on(archive.write(RegionChunk::new(3, 3), compressed.clone())).unwrap());
    });

    c.bench_function("compress", |b| {
        b.iter(|| Archive::compress(&chunk).unwrap());
    });
}

criterion_group!(benches, archive_bench);
criterion_main!(benches);
