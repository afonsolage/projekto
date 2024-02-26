use bracket_noise::prelude::*;
use projekto_core::{
    chunk::{self, Chunk, ChunkStorage},
    voxel::{self, Kind},
};

/// Generates a new chunk filling it with [`ChunkKind`] randomly generated by seeded noise
pub fn generate_chunk(chunk: Chunk, chunk_kind: &mut ChunkStorage<voxel::Kind>) {
    // Find a better seed gen method
    let seed = 42;

    // TODO: Move this to a config per-biome
    let mut noise = FastNoise::seeded(seed as u64);
    noise.set_noise_type(NoiseType::SimplexFractal);
    noise.set_frequency(0.03);
    noise.set_fractal_type(FractalType::FBM);
    noise.set_fractal_octaves(3);
    noise.set_fractal_gain(0.9);
    noise.set_fractal_lacunarity(0.5);
    let world = chunk::to_world(chunk);

    for x in 0..chunk::X_AXIS_SIZE {
        for z in 0..chunk::Z_AXIS_SIZE {
            let n = noise.get_noise(world.x + x as f32, world.z + z as f32);
            let n = (n + 1.0) / 2.0;
            let height = chunk::Y_AXIS_SIZE as f32 / 2.0 + n * chunk::X_AXIS_SIZE as f32;

            let end = usize::min(height as usize, chunk::Y_AXIS_SIZE);

            for y in 0..end {
                // TODO: Check this in a biome settings
                // let kind = voxel::Kind::get_kind_with_height_source(end - 1, y);
                let kind = Kind::id(3); // Stone

                chunk_kind.set((x as i32, y as i32, z as i32).into(), kind);
            }
        }
    }
}
