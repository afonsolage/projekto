//! Resident-memory measurement harness for chunk storage.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

use projekto_core::{
    chunk::{self, ChunkStorage, ChunkStorageCache, ChunkStorageType},
    coords::{Chunk, ChunkVoxel},
    voxel::{FacesOcclusion, FacesSoftLight, Kind, Light},
};

/// Tags every heap allocation with its size so a storage's resident footprint can be measured
/// without reaching into its internals.
struct CountingAllocator;

static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);

// SAFETY: every call is forwarded unchanged to the system allocator; this type only keeps a
// running byte total and never dereferences or reinterprets the memory it is handed.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: `ptr` and `layout` are forwarded unchanged from the caller.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn live_bytes() -> isize {
    LIVE_BYTES.load(Ordering::Relaxed)
}

/// Returns the heap bytes that remain resident once `build` has run and returned.
fn resident_bytes<T>(build: impl FnOnce() -> T) -> isize {
    let before = live_bytes();
    let value = build();
    let after = live_bytes();
    std::hint::black_box(&value);
    after - before
}

/// Fills a storage with a deterministic pattern and reports its resident heap bytes:
/// the enum slot array (`empty`) plus the index arrays and palettes (`dynamic`).
fn report_storage<T: ChunkStorageType>(
    name: &str,
    mut value: impl FnMut(ChunkVoxel) -> T,
) -> isize {
    let empty = resident_bytes(ChunkStorage::<T>::default);
    let full = resident_bytes(|| {
        let mut storage = ChunkStorage::<T>::default();
        for voxel in chunk::voxels() {
            storage.set(voxel, value(voxel));
        }
        storage.pack();
        storage
    });

    println!(
        "  {name:<12} enum {empty:>8} B   dynamic {:>9} B   total {full:>9} B",
        full - empty
    );
    full
}

fn report_memory() {
    const RADIUS: isize = 32;
    let chunks = (2 * RADIUS + 1).pow(2);

    println!(
        "Resident heap per storage, one chunk = {} voxels:",
        Chunk::BUFFER_SIZE
    );

    // Deterministic, terrain-like fill so the numbers are comparable across changes.
    let kind = |v: ChunkVoxel| {
        if v.y < 60 {
            Kind::id(1)
        } else if v.y < 64 {
            Kind::id(2)
        } else {
            Kind::none()
        }
    };
    let light = |v: ChunkVoxel| Light::from(if v.y < 64 { 0 } else { 15 });
    let occlusion = |v: ChunkVoxel| {
        if v.y < 64 {
            FacesOcclusion::fully_occluded()
        } else {
            FacesOcclusion::default()
        }
    };
    let soft_light = |v: ChunkVoxel| FacesSoftLight::with_intensity(15 - (v.y % 16));

    let mut per_chunk = 0;
    let kind_bytes = report_storage::<Kind>("kind", kind);
    per_chunk += kind_bytes;
    let light_bytes = report_storage::<Light>("light", light);
    per_chunk += light_bytes;
    per_chunk += report_storage::<FacesOcclusion>("occlusion", occlusion);
    per_chunk += report_storage::<FacesSoftLight>("soft_light", soft_light);

    println!(
        "  {:<12} per chunk {:>8} B   radius {RADIUS} ({chunks} chunks) {:>12} B",
        "TOTAL",
        per_chunk,
        per_chunk * chunks
    );
    println!();

    report_cached_memory(kind_bytes, light_bytes, occlusion, soft_light, chunks);
}

fn report_cached_memory(
    kind_bytes: isize,
    light_bytes: isize,
    occlusion: impl Fn(ChunkVoxel) -> FacesOcclusion,
    soft_light: impl Fn(ChunkVoxel) -> FacesSoftLight,
    chunks: isize,
) {
    const CACHE_CHUNKS: usize = 32;

    let occlusion_cache = resident_bytes(|| {
        let mut cache = ChunkStorageCache::<FacesOcclusion>::new(CACHE_CHUNKS);
        for i in 0..CACHE_CHUNKS {
            let chunk = Chunk::new(i as i32, 0);
            let mut storage = ChunkStorage::default();
            for voxel in chunk::voxels() {
                storage.set(voxel, occlusion(voxel));
            }
            storage.pack();
            cache.insert(chunk, storage);
        }
        cache
    });

    let soft_light_cache = resident_bytes(|| {
        let mut cache = ChunkStorageCache::<FacesSoftLight>::new(CACHE_CHUNKS);
        for i in 0..CACHE_CHUNKS {
            let chunk = Chunk::new(i as i32, 0);
            let mut storage = ChunkStorage::default();
            for voxel in chunk::voxels() {
                storage.set(voxel, soft_light(voxel));
            }
            storage.pack();
            cache.insert(chunk, storage);
        }
        cache
    });

    let resident_per_chunk = kind_bytes + light_bytes;
    let cache_bytes = occlusion_cache + soft_light_cache;
    let total = resident_per_chunk * chunks + cache_bytes;

    println!("Cached residency model (kind + light resident, derived layers on demand):");
    println!(
        "  {:<34} {:>10} B",
        "kind + light per chunk", resident_per_chunk
    );
    println!(
        "  {:<34} {:>10} B",
        "occlusion cache (32 chunks)", occlusion_cache
    );
    println!(
        "  {:<34} {:>10} B",
        "soft_light cache (32 chunks)", soft_light_cache
    );
    println!("  {:<34} {:>10} B", "TOTAL radius 32 (incl. cache)", total);
    println!();
}

fn main() {
    report_memory();
}
