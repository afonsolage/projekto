//! Deterministic terrain generation.
//!
//! The shape is built from three 2D fields (continentalness, erosion and weirdness) combined
//! through monotone splines, plus a small detail field. Overhangs and caves come from 3D fields
//! sampled on a coarse lattice and interpolated per voxel.

mod config;
mod noise;
mod spline;
mod surface;

pub use config::{WorldGenConfig, WorldGenConfigError};

use bevy::math::Vec3;
use projekto_core::{
    chunk::ChunkStorage,
    coords::{Chunk, ChunkVoxel},
    voxel::Kind,
};

use self::{
    config::CaveConfig,
    noise::{NoiseField, derive_seed},
    spline::Spline,
    surface::SurfaceRules,
};

const SEED_CONTINENTALNESS: u64 = 1;
const SEED_EROSION: u64 = 2;
const SEED_WEIRDNESS: u64 = 3;
const SEED_DETAIL: u64 = 4;
const SEED_OVERHANG: u64 = 5;
const SEED_CAVERN: u64 = 6;

/// Generates chunks from a validated [`WorldGenConfig`].
pub(crate) struct TerrainGenerator {
    config: WorldGenConfig,
    continentalness: NoiseField,
    erosion: NoiseField,
    weirdness: NoiseField,
    detail: NoiseField,
    overhang: Option<NoiseField>,
    cavern: Option<NoiseField>,
    base_height: Spline,
    relief_amplitude: Spline,
    surface: SurfaceRules,
}

impl TerrainGenerator {
    pub(crate) fn new(config: WorldGenConfig) -> Self {
        let seed = config.seed;
        let overhang = config
            .caves
            .enabled
            .then(|| NoiseField::new(&config.caves.overhang, derive_seed(seed, SEED_OVERHANG)));
        let cavern = config
            .caves
            .enabled
            .then(|| NoiseField::new(&config.caves.cavern, derive_seed(seed, SEED_CAVERN)));

        Self {
            continentalness: NoiseField::new(
                &config.fields.continentalness,
                derive_seed(seed, SEED_CONTINENTALNESS),
            ),
            erosion: NoiseField::new(&config.fields.erosion, derive_seed(seed, SEED_EROSION)),
            weirdness: NoiseField::new(&config.fields.weirdness, derive_seed(seed, SEED_WEIRDNESS)),
            detail: NoiseField::new(&config.fields.detail, derive_seed(seed, SEED_DETAIL)),
            overhang,
            cavern,
            base_height: Spline::new(&config.base_height),
            relief_amplitude: Spline::new(&config.relief_amplitude),
            surface: SurfaceRules::new(&config.surface),
            config,
        }
    }

    /// Surface height at world coordinates `(x, z)`.
    pub(crate) fn height(&self, x: f32, z: f32) -> f32 {
        let continentalness = self.continentalness.sample(x, z);
        let erosion = self.erosion.sample(x, z);
        let weirdness = self.weirdness.sample(x, z);

        let peaks_and_valleys = 1.0 - (3.0 * weirdness.abs() - 2.0).abs();
        let base = self.base_height.sample(continentalness);
        let amplitude = self.relief_amplitude.sample(erosion);
        let detail = self.detail.sample(x, z) * self.config.detail_strength;

        (base + peaks_and_valleys * amplitude + detail)
            .clamp(self.config.min_height as f32, self.config.max_height as f32)
    }

    /// Maximum surface height difference between the two neighbours on each horizontal axis.
    pub(crate) fn slope(&self, x: f32, z: f32) -> f32 {
        let dx = (self.height(x + 1.0, z) - self.height(x - 1.0, z)).abs();
        let dz = (self.height(x, z + 1.0) - self.height(x, z - 1.0)).abs();

        dx.max(dz) / 2.0
    }

    pub(crate) fn generate(&self, chunk: Chunk, kind: &mut ChunkStorage<Kind>) {
        let origin: Vec3 = chunk.into();
        let (y_min, y_max) = self.lattice_bounds();

        let overhang = self
            .overhang
            .as_ref()
            .map(|field| Lattice::new(field, origin, y_min, y_max, &self.config.caves));
        let cavern = self
            .cavern
            .as_ref()
            .map(|field| Lattice::new(field, origin, y_min, y_max, &self.config.caves));

        let mut heights = [[0.0f32; Chunk::X_AXIS_SIZE]; Chunk::Z_AXIS_SIZE];
        let mut slopes = [[0.0f32; Chunk::X_AXIS_SIZE]; Chunk::Z_AXIS_SIZE];

        for x in 0..Chunk::X_AXIS_SIZE {
            for z in 0..Chunk::Z_AXIS_SIZE {
                let world_x = origin.x + x as f32;
                let world_z = origin.z + z as f32;
                heights[z][x] = self.height(world_x, world_z);
                slopes[z][x] = self.slope(world_x, world_z);
            }
        }

        let deep = self.surface.deep();

        for x in 0..Chunk::X_AXIS_SIZE {
            for z in 0..Chunk::Z_AXIS_SIZE {
                let height = heights[z][x];
                let slope = slopes[z][x];
                let mut top = None;

                for y in 0..Chunk::Y_AXIS_SIZE {
                    let voxel = ChunkVoxel::new(x as u8, y as u8, z as u8);
                    if self.is_solid(height, origin, voxel, overhang.as_ref(), cavern.as_ref()) {
                        kind.set(voxel, deep);
                        top = Some(y);
                    }
                }

                let Some(top) = top else {
                    continue;
                };

                let mut depth = 0;
                let mut y = top as i32;
                while y >= 0 {
                    let voxel = ChunkVoxel::new(x as u8, y as u8, z as u8);
                    if kind.get(voxel).is_none() {
                        break;
                    }

                    kind.set(voxel, self.surface.kind(depth, slope));
                    depth += 1;

                    if depth > self.config.surface.subsurface_depth as i32 {
                        break;
                    }

                    y -= 1;
                }
            }
        }

        kind.pack();
    }

    /// Vertical range the 3D lattices must cover.
    ///
    /// The overhang field moves the surface by up to `overhang_strength`, so the sampling
    /// margin is the larger of `band` and the strength. Without this, a small `band` clamps the
    /// field over the top or bottom of the moved surface.
    fn lattice_bounds(&self) -> (i32, i32) {
        let margin = self
            .config
            .caves
            .band
            .max(self.config.caves.overhang_strength.ceil() as i32);
        let y_min = (self.config.min_height - margin).max(0);
        let y_max = (self.config.max_height + margin + 1).min(Chunk::Y_AXIS_SIZE as i32);

        (y_min, y_max)
    }

    fn is_solid(
        &self,
        height: f32,
        origin: Vec3,
        voxel: ChunkVoxel,
        overhang: Option<&Lattice>,
        cavern: Option<&Lattice>,
    ) -> bool {
        let world_x = origin.x + f32::from(voxel.x);
        let world_y = f32::from(voxel.y);
        let world_z = origin.z + f32::from(voxel.z);

        let mut surface = height;
        if let Some(overhang) = overhang {
            surface +=
                overhang.sample(world_x, world_y, world_z) * self.config.caves.overhang_strength;
        }

        if world_y >= surface {
            return false;
        }

        if world_y < height
            && let Some(cavern) = cavern
            && cavern.sample(world_x, world_y, world_z) > self.config.caves.cavern_threshold
        {
            return false;
        }

        true
    }

    #[cfg(test)]
    pub(crate) fn config(&self) -> &WorldGenConfig {
        &self.config
    }
}

/// A 3D noise field sampled on a coarse grid and interpolated trilinearly.
///
/// The grid is aligned to world coordinates and covers the chunk plus one cell, so neighbouring
/// chunks interpolate the same values at their shared border.
struct Lattice {
    origin: [i32; 3],
    size: [usize; 3],
    cell: [f32; 3],
    values: Vec<f32>,
}

impl Lattice {
    fn new(field: &NoiseField, origin: Vec3, y_min: i32, y_max: i32, config: &CaveConfig) -> Self {
        let cell_xz = config.cell_xz as i32;
        let cell_y = config.cell_y as i32;
        let x0 = origin.x as i32;
        let y0 = y_min.div_euclid(cell_y) * cell_y;
        let z0 = origin.z as i32;
        let nx = Chunk::X_AXIS_SIZE as i32 / cell_xz + 1;
        let nz = Chunk::Z_AXIS_SIZE as i32 / cell_xz + 1;
        // `i32::div_ceil` is still unstable on the pinned toolchain.
        let ny = ((y_max - y0 + cell_y - 1) / cell_y + 1).max(2);

        let mut values = Vec::with_capacity((nx * ny * nz) as usize);

        for y in 0..ny {
            for z in 0..nz {
                for x in 0..nx {
                    values.push(field.sample_3d(
                        (x0 + x * cell_xz) as f32,
                        (y0 + y * cell_y) as f32,
                        (z0 + z * cell_xz) as f32,
                    ));
                }
            }
        }

        Self {
            origin: [x0, y0, z0],
            size: [nx as usize, ny as usize, nz as usize],
            cell: [cell_xz as f32, cell_y as f32, cell_xz as f32],
            values,
        }
    }

    fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        let (ix, tx) = self.axis(0, x);
        let (iy, ty) = self.axis(1, y);
        let (iz, tz) = self.axis(2, z);

        let c000 = self.at(ix, iy, iz);
        let c100 = self.at(ix + 1, iy, iz);
        let c010 = self.at(ix, iy + 1, iz);
        let c110 = self.at(ix + 1, iy + 1, iz);
        let c001 = self.at(ix, iy, iz + 1);
        let c101 = self.at(ix + 1, iy, iz + 1);
        let c011 = self.at(ix, iy + 1, iz + 1);
        let c111 = self.at(ix + 1, iy + 1, iz + 1);

        let x00 = lerp(c000, c100, tx);
        let x10 = lerp(c010, c110, tx);
        let x01 = lerp(c001, c101, tx);
        let x11 = lerp(c011, c111, tx);

        let z0 = lerp(x00, x01, tz);
        let z1 = lerp(x10, x11, tz);

        lerp(z0, z1, ty)
    }

    fn axis(&self, axis: usize, value: f32) -> (usize, f32) {
        let max = (self.size[axis] - 1) as f32;
        let position = ((value - self.origin[axis] as f32) / self.cell[axis]).clamp(0.0, max);
        let index = (position.floor() as usize).min(self.size[axis] - 2);

        (index, position - index as f32)
    }

    fn at(&self, x: usize, y: usize, z: usize) -> f32 {
        self.values[(y * self.size[2] + z) * self.size[0] + x]
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn generator(seed: u64) -> TerrainGenerator {
        let mut config = WorldGenConfig::load_default();
        config.seed = seed;
        TerrainGenerator::new(config)
    }

    fn generate(generator: &TerrainGenerator, chunk: Chunk) -> ChunkStorage<Kind> {
        let mut kind = ChunkStorage::default();
        generator.generate(chunk, &mut kind);
        kind
    }

    fn kind_counts(kind: &ChunkStorage<Kind>) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();

        for voxel in projekto_core::chunk::voxels() {
            let kind = kind.get(voxel);
            if kind.is_none() {
                continue;
            }
            *counts.entry(kind.name()).or_insert(0) += 1;
        }

        counts
    }

    #[test]
    fn height_stays_within_configured_bounds_and_varies() {
        let generator = generator(42);
        let min = generator.config().min_height as f32;
        let max = generator.config().max_height as f32;
        let mut sampled_min = f32::MAX;
        let mut sampled_max = f32::MIN;

        for step in 0..2048 {
            let x = (step % 64) as f32 * 17.0 - 512.0;
            let z = (step / 64) as f32 * 17.0 - 512.0;
            let height = generator.height(x, z);
            sampled_min = sampled_min.min(height);
            sampled_max = sampled_max.max(height);

            assert!(
                (min..=max).contains(&height),
                "height {height} outside {min}..={max} at ({x}, {z})"
            );
        }

        assert!(
            sampled_max - sampled_min > 5.0,
            "height field is nearly constant: {sampled_min}..{sampled_max}"
        );
    }

    #[test]
    fn height_is_continuous_across_a_chunk_border() {
        let generator = generator(42);

        for step in 0..32 {
            let z = step as f32 * 7.3;
            let before = generator.height(Chunk::X_AXIS_SIZE as f32 - 0.01, z);
            let after = generator.height(Chunk::X_AXIS_SIZE as f32 + 0.01, z);

            assert!(
                (before - after).abs() < 1.0,
                "height jumped across the border: {before} -> {after}"
            );
        }
    }

    #[test]
    fn same_seed_generates_the_same_chunk() {
        let generator = generator(42);
        let a = generate(&generator, Chunk::new(0, 0));
        let b = generate(&generator, Chunk::new(0, 0));

        for voxel in projekto_core::chunk::voxels() {
            assert_eq!(a.get(voxel), b.get(voxel), "voxel {voxel:?} differs");
        }
    }

    #[test]
    fn different_seeds_generate_different_chunks() {
        let a = generate(&generator(42), Chunk::new(0, 0));
        let b = generate(&generator(43), Chunk::new(0, 0));

        let differences = projekto_core::chunk::voxels()
            .filter(|&voxel| a.get(voxel) != b.get(voxel))
            .count();

        assert!(differences > 1_000, "only {differences} voxels differ");
    }

    #[test]
    fn surface_has_grass_dirt_and_rock() {
        let generator = generator(42);
        let mut counts = BTreeMap::new();

        for chunk in [Chunk::new(0, 0), Chunk::new(1, 0), Chunk::new(0, 1)] {
            for (kind, count) in kind_counts(&generate(&generator, chunk)) {
                *counts.entry(kind).or_insert(0usize) += count;
            }
        }

        assert!(
            *counts.get("Grass").unwrap_or(&0) > 0,
            "no grass: {counts:?}"
        );
        assert!(*counts.get("Dirt").unwrap_or(&0) > 0, "no dirt: {counts:?}");
        assert!(*counts.get("Rock").unwrap_or(&0) > 0, "no rock: {counts:?}");
    }

    #[test]
    fn caves_carve_below_the_surface() {
        let generator = generator(42);
        let mut carved = 0usize;
        let mut solid_below = 0usize;

        for chunk in [Chunk::new(0, 0), Chunk::new(1, 0), Chunk::new(0, 1)] {
            let kind = generate(&generator, chunk);
            let origin: Vec3 = chunk.into();

            for x in 0..Chunk::X_AXIS_SIZE {
                for z in 0..Chunk::Z_AXIS_SIZE {
                    let height = generator.height(origin.x + x as f32, origin.z + z as f32);
                    let start = (height as usize).saturating_sub(8);

                    for y in 1..start {
                        let voxel = ChunkVoxel::new(x as u8, y as u8, z as u8);
                        if kind.get(voxel).is_none() {
                            carved += 1;
                        } else {
                            solid_below += 1;
                        }
                    }
                }
            }
        }

        assert!(carved > 0, "no caves carved");
        assert!(
            carved * 2 < solid_below,
            "caves carved too much: {carved} carved, {solid_below} solid"
        );
    }

    #[test]
    fn overhang_strength_moves_the_surface_up() {
        let mut config = WorldGenConfig::load_default();
        config.caves.overhang_strength = 24.0;
        config.caves.cavern_threshold = 1.0;
        let generator = TerrainGenerator::new(config);

        let kind = generate(&generator, Chunk::new(0, 0));
        let origin: Vec3 = Chunk::new(0, 0).into();
        let lifted = (0..Chunk::X_AXIS_SIZE).any(|x| {
            (0..Chunk::Z_AXIS_SIZE).any(|z| {
                let height = generator.height(origin.x + x as f32, origin.z + z as f32);
                let top = (0..=Chunk::Y_END)
                    .rev()
                    .find(|&y| !kind.get(ChunkVoxel::new(x as u8, y, z as u8)).is_none())
                    .map(|y| y as f32);

                top.is_some_and(|top| top > height + 4.0)
            })
        });

        assert!(lifted, "overhang field never lifted the surface");
    }

    #[test]
    fn lattice_is_continuous_across_chunk_borders() {
        let generator = generator(42);
        let field = generator
            .overhang
            .as_ref()
            .expect("default config enables caves");
        let config = &generator.config().caves;
        let (y_min, y_max) = generator.lattice_bounds();

        // (left chunk, right chunk, shared world x, shared world z)
        let cases = [
            (Chunk::new(0, 0), Chunk::new(1, 0), 16.0, 8.0),
            (Chunk::new(0, 0), Chunk::new(0, 1), 8.0, 16.0),
            (Chunk::new(-1, 0), Chunk::new(0, 0), 0.0, 8.0),
            (Chunk::new(0, -1), Chunk::new(0, 0), 8.0, 0.0),
        ];

        for (left_chunk, right_chunk, x, z) in cases {
            let left = Lattice::new(field, left_chunk.into(), y_min, y_max, config);
            let right = Lattice::new(field, right_chunk.into(), y_min, y_max, config);

            for step in 0..32 {
                let y = step as f32 * 7.0 + 1.0;
                let a = left.sample(x, y, z);
                let b = right.sample(x, y, z);

                assert!(
                    (a - b).abs() < 1e-5,
                    "lattice differs at {left_chunk:?}/{right_chunk:?} border: {a} != {b}"
                );
            }
        }
    }

    #[test]
    fn lattice_covers_the_overhang_range() {
        let mut config = WorldGenConfig::load_default();
        config.caves.band = 0;
        config.caves.overhang_strength = 24.0;
        config.max_height = 200;
        let generator = TerrainGenerator::new(config);
        let (y_min, y_max) = generator.lattice_bounds();
        let field = generator.overhang.as_ref().expect("caves are enabled");
        let config = &generator.config().caves;
        let lattice = Lattice::new(field, Chunk::new(0, 0).into(), y_min, y_max, config);

        let bottom = lattice.origin[1];
        let top = bottom + (lattice.size[1] as i32 - 1) * lattice.cell[1] as i32;

        assert!(
            bottom <= y_min,
            "lattice bottom {bottom} does not cover {y_min}"
        );
        assert!(top >= y_max, "lattice top {top} does not cover {y_max}");
        assert!(
            y_max >= 200 + 24,
            "lattice bounds do not include the overhang lift: {y_max}"
        );
    }

    #[test]
    fn lattice_matches_the_field_on_grid_points() {
        let generator = generator(42);
        let field = generator
            .overhang
            .as_ref()
            .expect("default config enables caves");
        let config = &generator.config().caves;
        let lattice = Lattice::new(
            field,
            Chunk::new(0, 0).into(),
            0,
            Chunk::Y_AXIS_SIZE as i32,
            config,
        );

        for x in (0..=Chunk::X_AXIS_SIZE).step_by(config.cell_xz as usize) {
            for z in (0..=Chunk::Z_AXIS_SIZE).step_by(config.cell_xz as usize) {
                for y in 0..4 {
                    let world_y = y as f32 * config.cell_y as f32;
                    let expected = field.sample_3d(x as f32, world_y, z as f32);
                    let actual = lattice.sample(x as f32, world_y, z as f32);

                    assert!(
                        (expected - actual).abs() < 1e-5,
                        "grid point ({x}, {world_y}, {z}) differs: {expected} != {actual}"
                    );
                }
            }
        }
    }
}
