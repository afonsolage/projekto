//! Renders the generated terrain for a quick visual check.
//!
//! Writes three images to `debug_reports/terrain-map` (or the second argument):
//! - `height.pgm`: top-down surface height, normalised between the map minimum and maximum.
//! - `surface.ppm`: top-down surface kind colour.
//! - `slice.ppm`: vertical x/y slice at world z = 0, coloured by kind.
//!
//! Usage: `cargo run -p projekto_server --example terrain_map -- [radius] [out_dir]`

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use projekto_core::{
    coords::{Chunk, ChunkVoxel},
    voxel::Kind,
};
use projekto_server::genesis::{ChunkCreation, GenesisServer};

const X_SIZE: usize = Chunk::X_AXIS_SIZE;
const Y_SIZE: usize = Chunk::Y_AXIS_SIZE;

fn main() {
    let mut args = std::env::args().skip(1);
    let radius: i32 = args
        .next()
        .map(|value| value.parse().expect("radius must be an integer"))
        .unwrap_or(8);
    let out_dir = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "debug_reports/terrain-map".to_string()),
    );

    let threads = std::env::var("PROJEKTO_TERRAIN_MAP_THREADS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|threads| threads.get())
                .unwrap_or(1)
        });

    let chunks = generate(radius, threads);
    let width = (radius * 2 + 1) as usize * X_SIZE;

    let (height, surface, height_min, height_max) = top_down(&chunks, radius, width);
    let height_pgm = normalise(&height, height_min, height_max);
    let slice = vertical_slice(&chunks, radius, width);

    fs::create_dir_all(&out_dir).expect("create terrain map output directory");
    write_pgm(&out_dir.join("height.pgm"), width, width, &height_pgm);
    write_ppm(&out_dir.join("surface.ppm"), width, width, &surface);
    write_ppm(&out_dir.join("slice.ppm"), width, Y_SIZE, &slice);

    println!(
        "terrain map: radius {radius} ({width}x{width} columns), height {height_min}..{height_max}"
    );
    println!("wrote {}", out_dir.join("height.pgm").display());
    println!("wrote {}", out_dir.join("surface.ppm").display());
    println!("wrote {}", out_dir.join("slice.ppm").display());
}

fn generate(radius: i32, threads: usize) -> HashMap<Chunk, ChunkCreation> {
    let server = GenesisServer::new(threads);
    let requested: Vec<Chunk> = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |z| Chunk::new(x, z)))
        .collect();

    let mut tasks: Vec<_> = requested
        .iter()
        .map(|&chunk| server.generate(chunk))
        .collect();
    let mut chunks = HashMap::with_capacity(requested.len());

    while !tasks.is_empty() {
        tasks.retain_mut(|task| match task.try_get_result() {
            Some(result) => {
                chunks.insert(task.chunk(), result.expect("genesis task failed"));
                false
            }
            None => true,
        });

        if !tasks.is_empty() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    chunks
}

fn top_down(
    chunks: &HashMap<Chunk, ChunkCreation>,
    radius: i32,
    width: usize,
) -> (Vec<u8>, Vec<[u8; 3]>, i32, i32) {
    let mut height = vec![0u8; width * width];
    let mut surface = vec![[0u8; 3]; width * width];
    let mut min = i32::MAX;
    let mut max = i32::MIN;

    for (index, (x, z)) in (0..width)
        .flat_map(|x| (0..width).map(move |z| (x, z)))
        .enumerate()
    {
        let world_x = x as i32 - radius * X_SIZE as i32;
        let world_z = z as i32 - radius * X_SIZE as i32;
        let Some(top) = top_voxel(chunks, world_x, world_z) else {
            continue;
        };

        height[index] = top.y;
        surface[index] = kind_color(top.kind);
        min = min.min(top.y as i32);
        max = max.max(top.y as i32);
    }

    (height, surface, min, max)
}

fn vertical_slice(
    chunks: &HashMap<Chunk, ChunkCreation>,
    radius: i32,
    width: usize,
) -> Vec<[u8; 3]> {
    let mut pixels = vec![[0u8; 3]; width * Y_SIZE];

    for x in 0..width {
        let world_x = x as i32 - radius * X_SIZE as i32;
        for y in 0..Y_SIZE {
            let kind = kind_at(chunks, world_x, y as u8, 0);
            let row = Y_SIZE - 1 - y;
            pixels[row * width + x] = kind_color(kind);
        }
    }

    pixels
}

struct TopVoxel {
    y: u8,
    kind: Kind,
}

fn top_voxel(
    chunks: &HashMap<Chunk, ChunkCreation>,
    world_x: i32,
    world_z: i32,
) -> Option<TopVoxel> {
    let chunk = Chunk::new(
        world_x.div_euclid(X_SIZE as i32),
        world_z.div_euclid(X_SIZE as i32),
    );
    let local_x = world_x.rem_euclid(X_SIZE as i32) as u8;
    let local_z = world_z.rem_euclid(X_SIZE as i32) as u8;
    let creation = chunks.get(&chunk)?;

    (0..=Chunk::Y_END)
        .rev()
        .map(|y| (y, creation.kind.get(ChunkVoxel::new(local_x, y, local_z))))
        .find(|(_, kind)| !kind.is_none())
        .map(|(y, kind)| TopVoxel { y, kind })
}

fn kind_at(chunks: &HashMap<Chunk, ChunkCreation>, world_x: i32, y: u8, world_z: i32) -> Kind {
    let chunk = Chunk::new(
        world_x.div_euclid(X_SIZE as i32),
        world_z.div_euclid(X_SIZE as i32),
    );
    let local_x = world_x.rem_euclid(X_SIZE as i32) as u8;
    let local_z = world_z.rem_euclid(X_SIZE as i32) as u8;

    chunks
        .get(&chunk)
        .map(|creation| creation.kind.get(ChunkVoxel::new(local_x, y, local_z)))
        .unwrap_or_else(Kind::none)
}

fn kind_color(kind: Kind) -> [u8; 3] {
    match u16::from(kind) {
        0 => [0, 0, 0],
        1 => [134, 96, 67],
        2 => [95, 159, 53],
        3 => [128, 128, 128],
        4 => [255, 220, 80],
        _ => [255, 0, 255],
    }
}

fn normalise(values: &[u8], min: i32, max: i32) -> Vec<u8> {
    let range = (max - min).max(1) as f32;
    values
        .iter()
        .map(|&value| (((value as i32 - min) as f32 / range) * 255.0).clamp(0.0, 255.0) as u8)
        .collect()
}

fn write_pgm(path: &Path, width: usize, height: usize, data: &[u8]) {
    let mut bytes = format!("P5\n{width} {height}\n255\n").into_bytes();
    bytes.extend_from_slice(data);
    fs::write(path, bytes).expect("write pgm");
}

fn write_ppm(path: &Path, width: usize, height: usize, data: &[[u8; 3]]) {
    let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in data {
        bytes.extend_from_slice(pixel);
    }
    fs::write(path, bytes).expect("write ppm");
}
