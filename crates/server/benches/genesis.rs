//! Genesis throughput and output statistics.
//!
//! Run with `cargo bench -p projekto_server --bench genesis`. The bench drives the same
//! `GenesisServer` the world uses, so the numbers include task-pool scheduling and light init.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use projekto_core::{
    chunk,
    coords::{Chunk, ChunkVoxel},
};
use projekto_server::genesis::{ChunkCreation, GenesisServer};

fn main() {
    let chunks = env_parse("PROJEKTO_GENESIS_BENCH_CHUNKS", 256usize);
    let threads = env_parse(
        "PROJEKTO_GENESIS_BENCH_THREADS",
        std::thread::available_parallelism()
            .map(|threads| threads.get())
            .unwrap_or(1),
    );

    let side = (chunks as f64).sqrt().ceil() as i32;
    let requested: Vec<Chunk> = (0..side)
        .flat_map(|x| (0..side).map(move |z| Chunk::new(x, z)))
        .take(chunks)
        .collect();

    let server = GenesisServer::new(threads);

    let started = Instant::now();
    let mut tasks: Vec<_> = requested
        .iter()
        .map(|&chunk| server.generate(chunk))
        .collect();
    let mut results = Vec::with_capacity(requested.len());

    while !tasks.is_empty() {
        tasks.retain_mut(|task| match task.try_get_result() {
            Some(result) => {
                results.push(result.expect("genesis task failed"));
                false
            }
            None => true,
        });

        if !tasks.is_empty() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let elapsed = started.elapsed();
    assert!(
        !results.is_empty(),
        "no chunks were generated; PROJEKTO_GENESIS_BENCH_CHUNKS must be positive"
    );
    let per_chunk = elapsed / results.len() as u32;
    let stats = collect_stats(&results);

    println!(
        "genesis: {} chunks on {} threads in {:.3} s ({:.2} ms/chunk)",
        results.len(),
        threads,
        elapsed.as_secs_f64(),
        per_chunk.as_secs_f64() * 1000.0
    );
    println!(
        "height: min={} mean={:.1} max={}",
        stats.height_min, stats.height_mean, stats.height_max
    );
    println!(
        "kinds: {}",
        stats
            .kinds
            .iter()
            .map(|(kind, count)| format!("{kind}={count}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
}

struct Stats {
    height_min: i32,
    height_max: i32,
    height_mean: f64,
    kinds: BTreeMap<&'static str, usize>,
}

fn collect_stats(results: &[ChunkCreation]) -> Stats {
    let mut height_min = i32::MAX;
    let mut height_max = i32::MIN;
    let mut height_sum = 0i64;
    let mut columns = 0usize;
    let mut kinds = BTreeMap::new();

    for ChunkCreation { kind, .. } in results {
        for voxel in chunk::voxels() {
            *kinds.entry(kind.get(voxel).name()).or_insert(0usize) += 1;
        }

        for x in 0..=Chunk::X_END {
            for z in 0..=Chunk::Z_END {
                let top = (0..=Chunk::Y_END)
                    .rev()
                    .find(|&y| !kind.get(ChunkVoxel::new(x, y, z)).is_none())
                    .map(|y| y as i32)
                    .unwrap_or(-1);

                height_min = height_min.min(top);
                height_max = height_max.max(top);
                height_sum += top as i64;
                columns += 1;
            }
        }
    }

    Stats {
        height_min,
        height_max,
        height_mean: height_sum as f64 / columns as f64,
        kinds,
    }
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
