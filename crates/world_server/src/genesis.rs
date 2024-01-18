use bevy_math::IVec3;
use bracket_noise::prelude::*;
use projekto_core::{
    chunk::{self, ChunkKind, ChunkLight},
    voxel,
};

pub struct GeneratedChunk {
    pub kind: ChunkKind,
    pub light: ChunkLight,
}

/// Generates a new chunk filling it with [`ChunkKind`] randomly generated by seeded noise
pub fn generate_chunk(local: IVec3) -> GeneratedChunk {
    // TODO: Move this to a config per-biome
    let mut noise = FastNoise::seeded(15);
    noise.set_noise_type(NoiseType::SimplexFractal);
    noise.set_frequency(0.03);
    noise.set_fractal_type(FractalType::FBM);
    noise.set_fractal_octaves(3);
    noise.set_fractal_gain(0.9);
    noise.set_fractal_lacunarity(0.5);
    let world = chunk::to_world(local);

    let mut chunk_kind = ChunkKind::default();
    let mut chunk_light = ChunkLight::default();

    for x in 0..chunk::X_AXIS_SIZE {
        for z in 0..chunk::Z_AXIS_SIZE {
            chunk_light.set(
                (x as i32, chunk::Y_END, z as i32).into(),
                voxel::Light::natural(voxel::Light::MAX_NATURAL_INTENSITY),
            );

            let h = noise.get_noise(world.x + x as f32, world.z + z as f32);
            let world_height = ((h + 1.0) / 2.0) * (chunk::X_AXIS_SIZE * 2) as f32;

            let height_local = world_height - world.y;

            if height_local < f32::EPSILON {
                continue;
            }

            let end = usize::min(height_local as usize, chunk::Y_AXIS_SIZE);

            for y in 0..end {
                // TODO: Check this in a biome settings
                let kind = voxel::Kind::get_kind_with_height_source(end - 1, y);

                chunk_kind.set((x as i32, y as i32, z as i32).into(), kind);
            }
        }
    }

    GeneratedChunk {
        kind: chunk_kind,
        light: chunk_light,
    }
}