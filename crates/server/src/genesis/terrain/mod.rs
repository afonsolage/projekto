//! Deterministic terrain generation.
//!
//! Low-frequency turbulence warps the shape sampling position, a landform mask blends a plains and
//! a mountain height profile, and an upheaval field adds tectonic uplift. Erosion controls a small
//! peaks-and-valleys relief term, and temperature and humidity select biomes and blend their shape
//! parameters (height offset, relief multiplier, terrace strength, cave density and overhang
//! strength) over a transition band. Each biome supplies its own surface materials; overhangs and
//! caves come from 3D fields sampled on a coarse lattice and interpolated per voxel.

mod biome;
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
    biome::BiomeTable,
    config::CaveConfig,
    noise::{NoiseField, derive_seed},
    spline::Spline,
};

const SEED_CONTINENTALNESS: u64 = 1;
const SEED_EROSION: u64 = 2;
const SEED_WEIRDNESS: u64 = 3;
const SEED_DETAIL: u64 = 4;
const SEED_OVERHANG: u64 = 5;
const SEED_CAVERN: u64 = 6;
const SEED_TEMPERATURE: u64 = 7;
const SEED_HUMIDITY: u64 = 8;
const SEED_LANDFORM: u64 = 9;
const SEED_UPHEAVAL: u64 = 10;
const SEED_TURBULENCE_X: u64 = 11;
const SEED_TURBULENCE_Z: u64 = 12;

/// Generates chunks from a validated [`WorldGenConfig`].
pub(crate) struct TerrainGenerator {
    config: WorldGenConfig,
    continentalness: NoiseField,
    erosion: NoiseField,
    weirdness: NoiseField,
    detail: NoiseField,
    overhang: Option<NoiseField>,
    cavern: Option<NoiseField>,
    landform: NoiseField,
    upheaval: NoiseField,
    turbulence_x: NoiseField,
    turbulence_z: NoiseField,
    landform_plains: Spline,
    landform_mountains: Spline,
    upheaval_lift: Spline,
    relief_amplitude: Spline,
    temperature: NoiseField,
    humidity: NoiseField,
    biomes: BiomeTable,
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
            landform: NoiseField::new(&config.fields.landform, derive_seed(seed, SEED_LANDFORM)),
            upheaval: NoiseField::new(&config.fields.upheaval, derive_seed(seed, SEED_UPHEAVAL)),
            turbulence_x: NoiseField::new(
                &config.fields.turbulence_x,
                derive_seed(seed, SEED_TURBULENCE_X),
            ),
            turbulence_z: NoiseField::new(
                &config.fields.turbulence_z,
                derive_seed(seed, SEED_TURBULENCE_Z),
            ),
            landform_plains: Spline::new(&config.landform_plains),
            landform_mountains: Spline::new(&config.landform_mountains),
            upheaval_lift: Spline::new(&config.upheaval_lift),
            relief_amplitude: Spline::new(&config.relief_amplitude),
            temperature: NoiseField::new(
                &config.fields.temperature,
                derive_seed(seed, SEED_TEMPERATURE),
            ),
            humidity: NoiseField::new(&config.fields.humidity, derive_seed(seed, SEED_HUMIDITY)),
            biomes: BiomeTable::new(&config.biomes, config.caves.overhang_strength),
            config,
        }
    }

    /// Surface height at world coordinates `(x, z)`.
    #[cfg(test)]
    pub(crate) fn height(&self, x: f32, z: f32) -> f32 {
        let mut weights = vec![0.0; self.biomes.len()];
        self.sample_column(x, z, &mut weights).height
    }

    /// Samples one world column: shape fields, climate fields, blend weights and height.
    fn sample_column(&self, x: f32, z: f32, weights: &mut [f32]) -> ColumnSample {
        // Warp the shape fields with a low-frequency vector field, so ridges, valleys and
        // landform boundaries do not line up with the underlying noise lattice.
        let warp_x = self.turbulence_x.sample(x, z) * self.config.warp_strength;
        let warp_z = self.turbulence_z.sample(x, z) * self.config.warp_strength;
        let shape_x = x + warp_x;
        let shape_z = z + warp_z;

        let continentalness = self.continentalness.sample(shape_x, shape_z);
        let erosion = self.erosion.sample(shape_x, shape_z);
        let weirdness = self.weirdness.sample(shape_x, shape_z);
        let landform = self.landform.sample(shape_x, shape_z);
        let upheaval = self.upheaval.sample(shape_x, shape_z);
        let detail = self.detail.sample(x, z) * self.config.detail_strength;
        let temperature = self.temperature.sample(x, z);
        let humidity = self.humidity.sample(x, z);

        let _ = self
            .biomes
            .weights_into(temperature, humidity, erosion, weights);
        let jitter = material_jitter(
            x,
            z,
            self.config.seed,
            self.config.biomes.material_blend_cell,
        );
        let surface_biome = self.biomes.surface_biome(weights, jitter);
        let shape = self.biomes.blended_shape(weights);

        // Blend two vertical profiles with the landform mask. The plains profile is low and
        // smooth; the mountain profile is much taller and steeper over the same continentalness.
        let weight = landform_weight(landform);
        let plains = self.landform_plains.sample(continentalness);
        let mountains = self.landform_mountains.sample(continentalness);
        let profile = plains + (mountains - plains) * weight;

        // Tectonic uplift raises long, sloped plains into cliffs and mountain fronts. The
        // coefficients scale it from 40% in pure plains to 100% in pure mountains, so lowlands
        // are not lifted uniformly.
        let uplift = self.upheaval_lift.sample(upheaval) * (0.4 + 0.6 * weight);
        // Relief is scaled from 60% in pure plains to 140% in pure mountains for ridged flanks.
        let amplitude = self.relief_amplitude.sample(erosion) * (0.6 + 0.8 * weight);
        let peaks_and_valleys = 1.0 - (3.0 * weirdness.abs() - 2.0).abs();
        let raw = profile
            + uplift
            + peaks_and_valleys * amplitude * shape.relief_multiplier
            + detail
            + shape.height_offset;
        let terraced =
            raw + shape.terrace_strength * (terrace(raw, self.config.biomes.terrace_step) - raw);
        let height = soft_clamp_height(
            terraced,
            self.config.min_height as f32,
            self.config.max_height as f32,
        );

        ColumnSample {
            height,
            surface_biome,
            overhang_strength: shape.overhang_strength,
            cave_density: shape.cave_density,
        }
    }

    /// Builds the per-column cache for one chunk: an 18x18 halo and the 16x16 slopes.
    ///
    /// The halo gives every column plus its four neighbours one authoritative world-space sample,
    /// so height, slope, biome and 3D parameters are never recomputed from different field reads.
    fn samples(&self, chunk: Chunk) -> ChunkSamples {
        let origin: Vec3 = chunk.into();
        let mut weights = vec![0.0; self.biomes.len()];
        let mut halo = [[ColumnSample::default(); HALO]; HALO];

        for (z, row) in halo.iter_mut().enumerate() {
            for (x, sample) in row.iter_mut().enumerate() {
                let world_x = origin.x + x as f32 - 1.0;
                let world_z = origin.z + z as f32 - 1.0;
                *sample = self.sample_column(world_x, world_z, &mut weights);
            }
        }

        let mut slopes = [[0.0f32; Chunk::X_AXIS_SIZE]; Chunk::Z_AXIS_SIZE];

        for (z, row) in slopes.iter_mut().enumerate() {
            for (x, slope) in row.iter_mut().enumerate() {
                let dx = (halo[z + 1][x + 2].height - halo[z + 1][x].height).abs();
                let dz = (halo[z + 2][x + 1].height - halo[z][x + 1].height).abs();
                *slope = dx.max(dz) / 2.0;
            }
        }

        ChunkSamples { halo, slopes }
    }

    /// Maximum surface height difference between the two neighbours on each horizontal axis.
    #[cfg(test)]
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

        let samples = self.samples(chunk);
        let deep = self.biomes.deep();

        for x in 0..Chunk::X_AXIS_SIZE {
            for z in 0..Chunk::Z_AXIS_SIZE {
                let column = samples.column(x, z);
                let slope = samples.slope(x, z);
                let mut top = None;

                for y in 0..Chunk::Y_AXIS_SIZE {
                    let voxel = ChunkVoxel::new(x as u8, y as u8, z as u8);
                    if self.is_solid(column, origin, voxel, overhang.as_ref(), cavern.as_ref()) {
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

                    kind.set(
                        voxel,
                        self.biomes.surface(column.surface_biome, depth, slope),
                    );
                    depth += 1;

                    if depth > self.biomes.surface_depth(column.surface_biome) {
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
    /// The overhang field moves the surface by up to the global `overhang_strength`, which is also
    /// the validated ceiling for every biome shape, so the sampling margin is the larger of `band`
    /// and that strength. Without this, a small `band` clamps the field over the top or bottom of
    /// the moved surface.
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
        column: ColumnSample,
        origin: Vec3,
        voxel: ChunkVoxel,
        overhang: Option<&Lattice>,
        cavern: Option<&Lattice>,
    ) -> bool {
        let world_x = origin.x + f32::from(voxel.x);
        let world_y = f32::from(voxel.y);
        let world_z = origin.z + f32::from(voxel.z);

        let mut surface = column.height;
        if let Some(overhang) = overhang {
            surface += overhang.sample(world_x, world_y, world_z) * column.overhang_strength;
        }

        if world_y >= surface {
            return false;
        }

        if world_y < column.height
            && column.cave_density > 0.0
            && let Some(cavern) = cavern
            && cavern.sample(world_x, world_y, world_z) * column.cave_density
                > self.config.caves.cavern_threshold
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

/// Number of columns cached per axis for a chunk: the 16 chunk columns plus one of margin.
const HALO: usize = Chunk::X_AXIS_SIZE + 2;

/// Cached shape values for one world column.
#[derive(Clone, Copy, Default)]
struct ColumnSample {
    height: f32,
    surface_biome: usize,
    overhang_strength: f32,
    cave_density: f32,
}

/// Per-column cache for one chunk, including the halo used for slopes.
struct ChunkSamples {
    halo: [[ColumnSample; HALO]; HALO],
    slopes: [[f32; Chunk::X_AXIS_SIZE]; Chunk::Z_AXIS_SIZE],
}

impl ChunkSamples {
    fn column(&self, x: usize, z: usize) -> ColumnSample {
        self.halo[z + 1][x + 1]
    }

    fn slope(&self, x: usize, z: usize) -> f32 {
        self.slopes[z][x]
    }
}

/// Smooth transition between the plains and mountain height profiles.
fn landform_weight(landform: f32) -> f32 {
    let value = ((landform + 1.0) * 0.5).clamp(0.0, 1.0);

    value * value * (3.0 - 2.0 * value)
}

/// Deterministic, uniform jitter in `0..1` for a material-blend cell.
///
/// The jitter must be uniform for `BiomeTable::surface_biome` to pick materials with probability
/// proportional to their weights. A noise field would not be uniform and would bias the result.
/// Cells are `cell` blocks wide, so materials form patches instead of per-block speckle.
fn material_jitter(x: f32, z: f32, seed: u64, cell: u8) -> f32 {
    let cell = f32::from(cell.max(1));
    let grid_x = (x / cell).floor() as i32;
    let grid_z = (z / cell).floor() as i32;
    let mut nearest = f32::MAX;
    let mut jitter = 0.0;

    // Pick the closest jittered feature point of the surrounding 3x3 cells. Feature points keep
    // the hash uniform per cell but make the cell borders irregular instead of axis-aligned.
    for offset_x in -1..=1 {
        for offset_z in -1..=1 {
            let cell_x = grid_x + offset_x;
            let cell_z = grid_z + offset_z;
            let feature_x = (cell_x as f32 + cell_fraction(cell_x, cell_z, seed, 1)) * cell;
            let feature_z = (cell_z as f32 + cell_fraction(cell_x, cell_z, seed, 2)) * cell;
            let distance = (x - feature_x) * (x - feature_x) + (z - feature_z) * (z - feature_z);

            if distance < nearest {
                nearest = distance;
                jitter = cell_fraction(cell_x, cell_z, seed, 3);
            }
        }
    }

    jitter
}

fn cell_fraction(x: i32, z: i32, seed: u64, salt: u64) -> f32 {
    let mut value = (x as u32 as u64) | ((z as u32 as u64) << 32);

    value ^= seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;

    (value >> 40) as f32 / 16_777_216.0
}

/// Compresses the top of the height range instead of clamping it to a flat ceiling.
///
/// Heights above the last 16 blocks are mapped through a rational asymptote, so rare peaks lose
/// height progressively and approach `max_height` without ever forming a flat plateau.
fn soft_clamp_height(height: f32, min_height: f32, max_height: f32) -> f32 {
    let knee = max_height - 16.0;
    let margin = max_height - knee;
    let compressed = if height > knee {
        let excess = height - knee;

        knee + margin * excess / (excess + margin)
    } else {
        height
    };

    compressed.clamp(min_height, max_height)
}

/// Smooth staircase used by the biome terrace blend.
///
/// The curve is continuous at cell edges and its derivative with respect to `height` stays in
/// `0..=1.5`, so terraces add relief character without unbounded slopes.
fn terrace(height: f32, step: f32) -> f32 {
    let cells = height / step;
    let whole = cells.floor();
    let fraction = cells - whole;
    let smooth = fraction * fraction * (3.0 - 2.0 * fraction);

    (whole + smooth) * step
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
    use super::{
        config::{BiomeParameters, BiomeShapeConfig},
        *,
    };
    use std::collections::BTreeMap;

    fn generator(seed: u64) -> TerrainGenerator {
        let mut config = WorldGenConfig::load_default();
        config.seed = seed;
        TerrainGenerator::new(config)
    }

    /// Recomputes the terrain part of a column without going through `sample_column`.
    ///
    /// Returns `(base elevation, relief amplitude, detail, peaks_and_valleys)` so tests can build
    /// an independent expected height for a neutral biome shape.
    fn independent_terrain(generator: &TerrainGenerator, x: f32, z: f32) -> (f32, f32, f32, f32) {
        let warp_x = generator.turbulence_x.sample(x, z) * generator.config().warp_strength;
        let warp_z = generator.turbulence_z.sample(x, z) * generator.config().warp_strength;
        let shape_x = x + warp_x;
        let shape_z = z + warp_z;

        let continentalness = generator.continentalness.sample(shape_x, shape_z);
        let erosion = generator.erosion.sample(shape_x, shape_z);
        let weirdness = generator.weirdness.sample(shape_x, shape_z);
        let landform = generator.landform.sample(shape_x, shape_z);
        let upheaval = generator.upheaval.sample(shape_x, shape_z);

        let weight = landform_weight(landform);
        let plains = generator.landform_plains.sample(continentalness);
        let mountains = generator.landform_mountains.sample(continentalness);
        let profile = plains + (mountains - plains) * weight;
        let uplift = generator.upheaval_lift.sample(upheaval) * (0.4 + 0.6 * weight);
        let amplitude = generator.relief_amplitude.sample(erosion) * (0.6 + 0.8 * weight);
        let detail = generator.detail.sample(x, z) * generator.config().detail_strength;
        let peaks_and_valleys = 1.0 - (3.0 * weirdness.abs() - 2.0).abs();

        (profile + uplift, amplitude, detail, peaks_and_valleys)
    }

    fn terrain_erosion(generator: &TerrainGenerator, x: f32, z: f32) -> f32 {
        let warp_x = generator.turbulence_x.sample(x, z) * generator.config().warp_strength;
        let warp_z = generator.turbulence_z.sample(x, z) * generator.config().warp_strength;

        generator.erosion.sample(x + warp_x, z + warp_z)
    }

    fn surface_biome_at(generator: &TerrainGenerator, x: f32, z: f32) -> usize {
        let mut weights = vec![0.0; generator.biomes.len()];
        generator.biomes.weights_into(
            generator.temperature.sample(x, z),
            generator.humidity.sample(x, z),
            terrain_erosion(generator, x, z),
            &mut weights,
        );
        let jitter = material_jitter(
            x,
            z,
            generator.config().seed,
            generator.config().biomes.material_blend_cell,
        );

        generator.biomes.surface_biome(&weights, jitter)
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

    fn biome_index(generator: &TerrainGenerator, x: f32, z: f32) -> usize {
        generator.biomes.select(
            generator.temperature.sample(x, z),
            generator.humidity.sample(x, z),
            terrain_erosion(generator, x, z),
        )
    }

    fn expected_top_kind(generator: &TerrainGenerator, x: f32, z: f32) -> Kind {
        let biome = surface_biome_at(generator, x, z);
        let slope = generator.slope(x, z);

        generator.biomes.surface(biome, 0, slope)
    }

    fn top_kind(kind: &ChunkStorage<Kind>, x: u8, z: u8) -> Option<Kind> {
        (0..=Chunk::Y_END)
            .rev()
            .find(|&y| !kind.get(ChunkVoxel::new(x, y, z)).is_none())
            .map(|y| kind.get(ChunkVoxel::new(x, y, z)))
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
    fn landform_weight_is_continuous_and_covers_the_range() {
        let mut previous = landform_weight(-1.0);
        assert_eq!(previous, 0.0);

        for step in 0..=20 {
            let landform = -1.0 + step as f32 * 0.1;
            let weight = landform_weight(landform);

            assert!((0.0..=1.0).contains(&weight));
            assert!(weight >= previous, "landform weight is not monotone");
            previous = weight;
        }

        assert_eq!(previous, 1.0);
        assert!((landform_weight(0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn warp_changes_the_terrain() {
        let mut config = WorldGenConfig::load_default();
        config.warp_strength = 0.0;
        let unwarped = TerrainGenerator::new(config);

        let warped = generator(42);
        let mut differences = 0;

        for step in 0..64 {
            let x = (step % 8) as f32 * 47.0 - 160.0;
            let z = (step / 8) as f32 * 53.0 - 180.0;

            if (unwarped.height(x, z) - warped.height(x, z)).abs() > 1.0 {
                differences += 1;
            }
        }

        assert!(differences > 0, "domain warp never changed the terrain");
    }

    #[test]
    fn landform_profiles_change_the_terrain() {
        let mut flat_mountains = WorldGenConfig::load_default();
        flat_mountains.landform_mountains = flat_mountains.landform_plains.clone();
        let flat = TerrainGenerator::new(flat_mountains);

        let shaped = generator(42);
        let mut differences = 0;

        for step in 0..64 {
            let x = (step % 8) as f32 * 61.0 - 200.0;
            let z = (step / 8) as f32 * 43.0 - 140.0;

            if (flat.height(x, z) - shaped.height(x, z)).abs() > 1.0 {
                differences += 1;
            }
        }

        assert!(
            differences > 0,
            "mountain profile never changed the terrain"
        );
    }

    #[test]
    fn upheaval_lift_changes_the_terrain() {
        let mut no_uplift = WorldGenConfig::load_default();
        for (_, value) in &mut no_uplift.upheaval_lift.points {
            *value = 0.0;
        }
        let flat = TerrainGenerator::new(no_uplift);

        let uplifted = generator(42);
        let mut differences = 0;

        for step in 0..64 {
            let x = (step % 8) as f32 * 71.0 - 240.0;
            let z = (step / 8) as f32 * 37.0 - 120.0;

            if (flat.height(x, z) - uplifted.height(x, z)).abs() > 1.0 {
                differences += 1;
            }
        }

        assert!(differences > 0, "tectonic uplift never changed the terrain");
    }

    #[test]
    fn soft_clamp_compresses_peaks_without_flat_plateaus() {
        assert_eq!(soft_clamp_height(236.0, 70.0, 252.0), 236.0);

        let mut previous = soft_clamp_height(200.0, 70.0, 252.0);

        for step in 201..=400 {
            let height = soft_clamp_height(step as f32, 70.0, 252.0);

            assert!(height >= previous, "soft clamp is not monotone");
            assert!(
                height < 252.0,
                "soft clamp reached the hard ceiling at raw {step}: {height}"
            );
            previous = height;
        }
    }

    #[test]
    fn generator_compresses_peaks_instead_of_plateauing() {
        let mut config = WorldGenConfig::load_default();

        for (_, value) in &mut config.landform_mountains.points {
            *value = 340.0;
        }
        for (_, value) in &mut config.upheaval_lift.points {
            *value = 0.0;
        }

        let generator = TerrainGenerator::new(config);
        let mut compressed = 0;

        for step in 0..64 {
            let x = (step % 8) as f32 * 97.0 - 320.0;
            let z = (step / 8) as f32 * 89.0 - 280.0;
            let height = generator.height(x, z);

            assert!(
                height < 252.0,
                "height reached the hard ceiling at ({x}, {z}): {height}"
            );
            if height > 236.0 {
                compressed += 1;
            }
        }

        assert!(compressed > 0, "no column exercised the soft-clamp branch");
    }

    #[test]
    fn material_jitter_is_uniform() {
        let cell = 12u8;
        let mut samples = 0usize;
        let mut below_half = 0usize;
        let mut below_ninety = 0usize;
        let mut sum = 0.0f64;

        for cell_x in -100..100 {
            for cell_z in -100..100 {
                let x = cell_x as f32 * f32::from(cell) + 0.37 * f32::from(cell);
                let z = cell_z as f32 * f32::from(cell) + 0.61 * f32::from(cell);
                let jitter = material_jitter(x, z, 42, cell) as f64;

                samples += 1;
                sum += jitter;
                if jitter < 0.5 {
                    below_half += 1;
                }
                if jitter < 0.9 {
                    below_ninety += 1;
                }
            }
        }

        let mean = sum / samples as f64;
        let half = below_half as f64 / samples as f64;
        let ninety = below_ninety as f64 / samples as f64;

        assert!((mean - 0.5).abs() < 0.01, "jitter mean is {mean}");
        assert!((half - 0.5).abs() < 0.02, "P(jitter < 0.5) is {half}");
        assert!((ninety - 0.9).abs() < 0.02, "P(jitter < 0.9) is {ninety}");
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
    fn surface_kinds_are_configured_biome_kinds() {
        let generator = generator(42);
        let config = generator.config();
        let mut counts = BTreeMap::new();

        for chunk in [
            Chunk::new(0, 0),
            Chunk::new(1, 0),
            Chunk::new(0, 1),
            Chunk::new(-1, -1),
        ] {
            for (kind, count) in kind_counts(&generate(&generator, chunk)) {
                *counts.entry(kind).or_insert(0usize) += count;
            }
        }

        let allowed = std::iter::once(config.biomes.steep.as_str())
            .chain(std::iter::once(config.biomes.deep.as_str()))
            .chain(config.biomes.entries.iter().flat_map(|biome| {
                [
                    biome.surface.top.as_str(),
                    biome.surface.subsurface.as_str(),
                ]
            }))
            .collect::<Vec<_>>();

        assert!(!counts.is_empty(), "no solid kinds generated");
        for name in counts.keys() {
            assert!(
                allowed.contains(name),
                "generated unconfigured kind {name}: {counts:?}"
            );
        }
        assert!(*counts.get("Rock").unwrap_or(&0) > 0, "no rock: {counts:?}");
    }

    #[test]
    fn biome_distribution_stays_within_bounds() {
        const SAMPLES: usize = 256;
        const STEP: usize = 32;

        for seed in [42, 43, 44] {
            let generator = generator(seed);
            let mut counts = vec![0usize; generator.config().biomes.entries.len()];

            for x in 0..SAMPLES {
                for z in 0..SAMPLES {
                    let index = biome_index(&generator, (x * STEP) as f32, (z * STEP) as f32);
                    counts[index] += 1;
                }
            }

            let total = SAMPLES * SAMPLES;
            let shares: Vec<f32> = counts
                .iter()
                .map(|count| *count as f32 / total as f32)
                .collect();
            println!(
                "seed {seed} biome shares: {}",
                shares
                    .iter()
                    .enumerate()
                    .map(|(index, share)| format!(
                        "{}={:.1}%",
                        generator.config().biomes.entries[index].name,
                        share * 100.0
                    ))
                    .collect::<Vec<_>>()
                    .join(" ")
            );

            for (index, share) in shares.iter().enumerate() {
                assert!(
                    (0.05..=0.40).contains(share),
                    "biome {} has share {:.1}% for seed {seed}",
                    generator.config().biomes.entries[index].name,
                    share * 100.0
                );
            }
        }
    }

    #[test]
    fn surface_biome_distribution_stays_within_bounds() {
        const SAMPLES: usize = 256;
        const STEP: usize = 32;

        for seed in [42, 43, 44] {
            let generator = generator(seed);
            let mut counts = vec![0usize; generator.biomes.len()];

            for x in 0..SAMPLES {
                for z in 0..SAMPLES {
                    let index = surface_biome_at(&generator, (x * STEP) as f32, (z * STEP) as f32);
                    counts[index] += 1;
                }
            }

            let total = SAMPLES * SAMPLES;

            for (index, count) in counts.iter().enumerate() {
                let share = *count as f32 / total as f32;

                assert!(
                    (0.05..=0.40).contains(&share),
                    "surface biome {} has share {:.1}% for seed {seed}",
                    generator.config().biomes.entries[index].name,
                    share * 100.0
                );
            }
        }
    }

    #[test]
    fn world_space_surface_matches_generated_chunks() {
        let generator = generator(42);

        for chunk in [
            Chunk::new(0, 0),
            Chunk::new(1, 0),
            Chunk::new(-1, 0),
            Chunk::new(0, 1),
            Chunk::new(0, -1),
        ] {
            let kind = generate(&generator, chunk);
            let samples = generator.samples(chunk);
            let origin: Vec3 = chunk.into();

            for x in 0..Chunk::X_AXIS_SIZE {
                for z in 0..Chunk::Z_AXIS_SIZE {
                    let world_x = origin.x + x as f32;
                    let world_z = origin.z + z as f32;
                    let column = samples.column(x, z);
                    let probe_height = generator.height(world_x, world_z);
                    let probe_slope = generator.slope(world_x, world_z);
                    let expected = expected_top_kind(&generator, world_x, world_z);
                    let actual = top_kind(&kind, x as u8, z as u8)
                        .unwrap_or_else(|| panic!("no solid voxel at ({world_x}, {world_z})"));

                    assert!(
                        (column.height - probe_height).abs() < 1e-3,
                        "cached height {} differs from probe {probe_height} at ({world_x}, {world_z})",
                        column.height
                    );
                    assert!(
                        (samples.slope(x, z) - probe_slope).abs() < 1e-3,
                        "cached slope {} differs from probe {probe_slope} at ({world_x}, {world_z})",
                        samples.slope(x, z)
                    );
                    assert_eq!(
                        column.surface_biome,
                        surface_biome_at(&generator, world_x, world_z),
                        "cached surface biome differs from the world-space probe at ({world_x}, {world_z})"
                    );
                    assert_eq!(
                        actual, expected,
                        "generated surface differs from world-space probe at ({world_x}, {world_z}) in {chunk:?}"
                    );
                }
            }
        }

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
    fn each_biome_generates_its_surface_sequence() {
        let mut config = WorldGenConfig::load_default();
        config.caves.enabled = false;
        for entry in &mut config.biomes.entries {
            entry.shape.overhang_strength = 0.0;
        }
        let generator = TerrainGenerator::new(config);
        let mut candidates = vec![None; generator.config().biomes.entries.len()];

        for x in 0..256 {
            for z in 0..256 {
                let world_x = (x * 16) as f32;
                let world_z = (z * 16) as f32;
                let index = surface_biome_at(&generator, world_x, world_z);

                if candidates[index].is_none()
                    && generator.slope(world_x, world_z) <= generator.config().biomes.max_slope
                {
                    candidates[index] = Some((world_x, world_z));
                }
            }
        }

        for (biome, candidate) in candidates.iter().enumerate() {
            let (world_x, world_z) = candidate.unwrap_or_else(|| {
                panic!(
                    "biome {} has no flat column in the scan",
                    generator.config().biomes.entries[biome].name
                )
            });
            let chunk = Chunk::new(
                (world_x as i32).div_euclid(Chunk::X_AXIS_SIZE as i32),
                (world_z as i32).div_euclid(Chunk::Z_AXIS_SIZE as i32),
            );
            let kind = generate(&generator, chunk);
            let x = (world_x as i32).rem_euclid(Chunk::X_AXIS_SIZE as i32) as u8;
            let z = (world_z as i32).rem_euclid(Chunk::Z_AXIS_SIZE as i32) as u8;
            let top = (0..=Chunk::Y_END)
                .rev()
                .find(|&y| !kind.get(ChunkVoxel::new(x, y, z)).is_none())
                .unwrap_or_else(|| panic!("no solid voxel at ({world_x}, {world_z})"));

            let slope = generator.slope(world_x, world_z);
            let depth = generator.biomes.surface_depth(biome);

            for offset in 0..=depth + 1 {
                let y = top as i32 - offset;
                assert!(
                    y >= 0,
                    "surface run leaves the chunk at ({world_x}, {world_z})"
                );
                assert_eq!(
                    kind.get(ChunkVoxel::new(x, y as u8, z)),
                    generator.biomes.surface(biome, offset, slope),
                    "biome {} surface mismatch at depth {offset} ({world_x}, {world_z})",
                    generator.config().biomes.entries[biome].name
                );
            }
        }
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
        for entry in &mut config.biomes.entries {
            entry.shape.overhang_strength = 24.0;
            entry.shape.cave_density = 0.0;
        }
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

    fn two_entry_config(transition: f32) -> WorldGenConfig {
        let mut config = WorldGenConfig::load_default();
        config.biomes.entries.truncate(2);
        config.biomes.transition = transition;

        for (index, entry) in config.biomes.entries.iter_mut().enumerate() {
            entry.parameters = BiomeParameters {
                temperature: if index == 0 { -0.5 } else { 0.5 },
                humidity: 0.0,
                erosion: 0.0,
            };
            entry.shape = BiomeShapeConfig {
                height_offset: if index == 0 { 20.0 } else { -20.0 },
                relief_multiplier: 1.0,
                terrace_strength: 0.0,
                cave_density: 1.0,
                overhang_strength: 6.0,
            };
        }

        config
    }

    fn weight_gap(generator: &TerrainGenerator, x: f32, z: f32) -> f32 {
        let mut weights = vec![0.0; generator.biomes.len()];
        generator.biomes.weights_into(
            generator.temperature.sample(x, z),
            generator.humidity.sample(x, z),
            terrain_erosion(generator, x, z),
            &mut weights,
        );

        weights[0] - weights[1]
    }

    fn boundary_point(generator: &TerrainGenerator) -> (f32, f32) {
        let z = 0.0;
        let mut low = -4096.0;
        let mut low_sign = weight_gap(generator, low, z).signum();

        while low < 4096.0 {
            let high = low + 4.0;
            let high_sign = weight_gap(generator, high, z).signum();

            if low_sign != high_sign {
                let mut inside = low;
                let mut outside = high;

                for _ in 0..80 {
                    let mid = (inside + outside) / 2.0;

                    if weight_gap(generator, mid, z).signum() == low_sign {
                        inside = mid;
                    } else {
                        outside = mid;
                    }
                }

                return ((inside + outside) / 2.0, z);
            }

            low = high;
            low_sign = high_sign;
        }

        panic!("no temperature boundary found in the scan");
    }

    fn shipped_boundary_columns(generator: &TerrainGenerator) -> (f32, f32, f32, usize, usize) {
        let z = 0.0;
        let mut weights = vec![0.0; generator.biomes.len()];
        let mut previous: Option<(f32, usize)> = None;

        for step in -2048..=2048 {
            let x = step as f32;
            let dominant = generator.biomes.weights_into(
                generator.temperature.sample(x, z),
                generator.humidity.sample(x, z),
                terrain_erosion(generator, x, z),
                &mut weights,
            );

            if let Some((previous_x, previous_dominant)) = previous
                && previous_dominant != dominant
            {
                return (previous_x, x, z, previous_dominant, dominant);
            }

            previous = Some((x, dominant));
        }

        panic!("no shipped biome boundary found in the scan");
    }

    #[test]
    fn height_has_no_jump_across_a_shipped_biome_boundary() {
        let generator = generator(42);
        let (left, right, z, first, second) = shipped_boundary_columns(&generator);
        let mut weights = vec![0.0; generator.biomes.len()];

        for (label, x) in [("left", left), ("right", right)] {
            generator.biomes.weights_into(
                generator.temperature.sample(x, z),
                generator.humidity.sample(x, z),
                terrain_erosion(&generator, x, z),
                &mut weights,
            );
            assert!(
                weights[first] > 0.05 && weights[second] > 0.05,
                "{label} boundary column has no blended weights: {weights:?}"
            );
        }

        let steps = 40;
        let mut previous = generator.height(left, z);

        for index in 1..=steps {
            let x = left + (right - left) * index as f32 / steps as f32;
            let height = generator.height(x, z);

            assert!(
                (height - previous).abs() < 0.2,
                "height jumped at x={x}: {previous} -> {height}"
            );
            previous = height;
        }
    }

    #[test]
    fn blended_height_matches_an_independent_formula() {
        let mut config = WorldGenConfig::load_default();

        for entry in &mut config.biomes.entries {
            entry.shape.terrace_strength = 0.0;
        }
        config.biomes.entries[0].shape.height_offset = 10.0;
        config.biomes.entries[0].shape.relief_multiplier = 1.5;
        config.biomes.entries[1].shape.height_offset = -6.0;
        config.biomes.entries[1].shape.relief_multiplier = 0.7;

        let generator = TerrainGenerator::new(config);
        let mut weights = vec![0.0; generator.biomes.len()];

        for step in 0..256 {
            let x = (step % 16) as f32 * 19.0 - 128.0;
            let z = (step / 16) as f32 * 23.0 - 128.0;
            let erosion = terrain_erosion(&generator, x, z);

            generator.biomes.weights_into(
                generator.temperature.sample(x, z),
                generator.humidity.sample(x, z),
                erosion,
                &mut weights,
            );
            let shape = generator.biomes.blended_shape(&weights);
            let (base, amplitude, detail, peaks_and_valleys) =
                independent_terrain(&generator, x, z);
            let raw = base
                + peaks_and_valleys * amplitude * shape.relief_multiplier
                + detail
                + shape.height_offset;
            let expected = soft_clamp_height(
                raw,
                generator.config().min_height as f32,
                generator.config().max_height as f32,
            );
            let actual = generator.height(x, z);

            assert!(
                (actual - expected).abs() < 1e-3,
                "independent formula differs at ({x}, {z}): {actual} != {expected}"
            );
        }
    }

    #[test]
    fn surface_materials_interleave_across_a_transition() {
        let generator = TerrainGenerator::new(two_entry_config(0.4));
        let (x, z) = boundary_point(&generator);
        let mut seen = [false; 2];
        let mut previous = None;
        let mut changes = 0;

        for step in -250..=250 {
            let sample = generator.sample_column(
                x + step as f32 * 2.0,
                z,
                &mut vec![0.0; generator.biomes.len()],
            );

            if sample.surface_biome < 2 {
                seen[sample.surface_biome] = true;
            }
            if let Some(previous) = previous
                && previous != sample.surface_biome
            {
                changes += 1;
            }
            previous = Some(sample.surface_biome);
        }

        assert!(
            seen[0] && seen[1],
            "surface materials did not interleave across the transition: {seen:?}"
        );
        assert!(
            changes >= 10,
            "surface materials changed only {changes} times across the transition"
        );
    }

    #[test]
    fn neutral_shapes_do_not_change_the_height() {
        let mut config = WorldGenConfig::load_default();
        for entry in &mut config.biomes.entries {
            entry.shape.height_offset = 0.0;
            entry.shape.relief_multiplier = 1.0;
            entry.shape.terrace_strength = 0.0;
            entry.shape.cave_density = 1.0;
            entry.shape.overhang_strength = config.caves.overhang_strength;
        }
        config.seed = 42;
        let generator = TerrainGenerator::new(config);
        let mut weights = vec![0.0; generator.biomes.len()];

        for step in 0..256 {
            let x = (step % 16) as f32 * 19.0 - 128.0;
            let z = (step / 16) as f32 * 23.0 - 128.0;
            let erosion = terrain_erosion(&generator, x, z);

            generator.biomes.weights_into(
                generator.temperature.sample(x, z),
                generator.humidity.sample(x, z),
                erosion,
                &mut weights,
            );
            let (base, amplitude, detail, peaks_and_valleys) =
                independent_terrain(&generator, x, z);
            let expected = soft_clamp_height(
                base + peaks_and_valleys * amplitude + detail,
                generator.config().min_height as f32,
                generator.config().max_height as f32,
            );

            assert!(
                (generator.height(x, z) - expected).abs() < 1e-6,
                "neutral shapes altered the height at ({x}, {z})"
            );
        }
    }

    #[test]
    fn height_has_no_jump_across_a_synthetic_biome_boundary() {
        let generator = TerrainGenerator::new(two_entry_config(0.15));
        let (x, z) = boundary_point(&generator);
        let before = generator.height(x - 1e-3, z);
        let after = generator.height(x + 1e-3, z);

        assert!(
            (before - after).abs() < 0.05,
            "height jumped across the boundary: {before} -> {after}"
        );

        let mut weights = vec![0.0; generator.biomes.len()];
        generator.biomes.weights_into(
            generator.temperature.sample(x, z),
            generator.humidity.sample(x, z),
            generator.erosion.sample(x, z),
            &mut weights,
        );
        assert!(
            (weights[0] - weights[1]).abs() < 5e-3,
            "boundary weights are not even: {weights:?}"
        );

        let mut previous = generator.height(x - 1.0, z);
        let mut offset = -1.0;

        while offset < 1.0 {
            offset += 0.01;
            let height = generator.height(x + offset, z);

            assert!(
                (height - previous).abs() < 0.1,
                "height jumped at offset {offset}: {previous} -> {height}"
            );
            previous = height;
        }
    }

    #[test]
    fn transition_sweep_keeps_the_boundary_continuous() {
        for transition in [0.1, 0.15, 0.5] {
            let generator = TerrainGenerator::new(two_entry_config(transition));
            let (x, z) = boundary_point(&generator);
            let before = generator.height(x - 1e-3, z);
            let after = generator.height(x + 1e-3, z);

            assert!(
                (before - after).abs() < 0.05,
                "transition {transition} left a boundary jump: {before} -> {after}"
            );
        }
    }

    #[test]
    fn cached_slope_matches_the_probe() {
        let generator = generator(42);

        for chunk in [
            Chunk::new(0, 0),
            Chunk::new(1, 0),
            Chunk::new(-1, 0),
            Chunk::new(0, 1),
            Chunk::new(0, -1),
        ] {
            let samples = generator.samples(chunk);
            let origin: Vec3 = chunk.into();

            for x in 0..Chunk::X_AXIS_SIZE {
                for z in 0..Chunk::Z_AXIS_SIZE {
                    let world_x = origin.x + x as f32;
                    let world_z = origin.z + z as f32;
                    let probe = generator.slope(world_x, world_z);

                    assert!(
                        (samples.slope(x, z) - probe).abs() < 1e-5,
                        "cached slope {} differs from probe {probe} at ({world_x}, {world_z})",
                        samples.slope(x, z)
                    );
                }
            }
        }
    }

    #[test]
    fn caves_disabled_top_matches_height() {
        let mut config = WorldGenConfig::load_default();
        config.caves.enabled = false;
        let generator = TerrainGenerator::new(config);

        for chunk in [Chunk::new(0, 0), Chunk::new(1, 0), Chunk::new(-1, 0)] {
            let kind = generate(&generator, chunk);
            let origin: Vec3 = chunk.into();

            for x in 0..Chunk::X_AXIS_SIZE {
                for z in 0..Chunk::Z_AXIS_SIZE {
                    let height = generator.height(origin.x + x as f32, origin.z + z as f32);
                    let expected = (height.ceil() as i32 - 1) as u8;
                    let top = (0..=Chunk::Y_END)
                        .rev()
                        .find(|&y| !kind.get(ChunkVoxel::new(x as u8, y, z as u8)).is_none())
                        .unwrap_or_else(|| panic!("no solid voxel at ({x}, {z}) in {chunk:?}"));

                    assert_eq!(top, expected, "top voxel differs from height at ({x}, {z})");
                }
            }
        }
    }

    #[test]
    fn cave_density_zero_removes_caverns() {
        let mut config = WorldGenConfig::load_default();
        config.caves.overhang_strength = 0.0;
        config.caves.cavern_threshold = 0.0;
        for entry in &mut config.biomes.entries {
            entry.shape.cave_density = 0.0;
            entry.shape.overhang_strength = 0.0;
        }
        let generator = TerrainGenerator::new(config);

        for chunk in [Chunk::new(0, 0), Chunk::new(1, 0)] {
            let kind = generate(&generator, chunk);
            let origin: Vec3 = chunk.into();

            for x in 0..Chunk::X_AXIS_SIZE {
                for z in 0..Chunk::Z_AXIS_SIZE {
                    let height = generator.height(origin.x + x as f32, origin.z + z as f32);

                    for y in 0..height as usize {
                        assert!(
                            !kind
                                .get(ChunkVoxel::new(x as u8, y as u8, z as u8))
                                .is_none(),
                            "cavern carved a zero-density column at ({x}, {y}, {z})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cave_density_increases_carved_volume() {
        fn carved(density: f32) -> usize {
            let mut config = WorldGenConfig::load_default();
            config.caves.overhang_strength = 0.0;
            for entry in &mut config.biomes.entries {
                entry.shape.cave_density = density;
                entry.shape.overhang_strength = 0.0;
            }
            let generator = TerrainGenerator::new(config);
            let mut carved = 0;

            for chunk in [Chunk::new(0, 0), Chunk::new(1, 0)] {
                let kind = generate(&generator, chunk);
                let origin: Vec3 = chunk.into();

                for x in 0..Chunk::X_AXIS_SIZE {
                    for z in 0..Chunk::Z_AXIS_SIZE {
                        let height = generator.height(origin.x + x as f32, origin.z + z as f32);
                        let start = (height as usize).saturating_sub(8);

                        for y in 1..start {
                            if kind
                                .get(ChunkVoxel::new(x as u8, y as u8, z as u8))
                                .is_none()
                            {
                                carved += 1;
                            }
                        }
                    }
                }
            }

            carved
        }

        assert_eq!(carved(0.0), 0, "zero density carved voxels");
        assert!(carved(2.0) > 0, "high density carved no voxels");
    }

    #[test]
    fn generator_blends_non_uniform_cave_and_overhang_strength() {
        let mut config = WorldGenConfig::load_default();
        config.biomes.entries.truncate(2);
        config.biomes.transition = 0.2;
        config.caves.overhang_strength = 18.0;

        for (index, entry) in config.biomes.entries.iter_mut().enumerate() {
            let first = index == 0;
            entry.parameters = BiomeParameters {
                temperature: if first { -0.5 } else { 0.5 },
                humidity: 0.0,
                erosion: 0.0,
            };
            entry.shape = BiomeShapeConfig {
                height_offset: 0.0,
                relief_multiplier: 1.0,
                terrace_strength: 0.0,
                cave_density: if first { 0.0 } else { 2.0 },
                overhang_strength: if first { 2.0 } else { 18.0 },
            };
        }

        let generator = TerrainGenerator::new(config);
        let (x, z) = boundary_point(&generator);
        let mut weights = vec![0.0; generator.biomes.len()];
        let sample = generator.sample_column(x, z, &mut weights);

        assert!(
            (weights[0] - weights[1]).abs() < 1e-2,
            "boundary weights are not even: {weights:?}"
        );
        assert!(
            (sample.cave_density - 1.0).abs() < 1e-2,
            "blended cave density {} should be the mean of 0 and 2",
            sample.cave_density
        );
        assert!(
            (sample.overhang_strength - 10.0).abs() < 1e-2,
            "blended overhang strength {} should be the mean of 2 and 18",
            sample.overhang_strength
        );
    }

    #[test]
    fn overhang_strength_zero_keeps_the_surface_at_height() {
        let mut config = WorldGenConfig::load_default();
        config.caves.cavern_threshold = 1.0;
        for entry in &mut config.biomes.entries {
            entry.shape.overhang_strength = 0.0;
            entry.shape.cave_density = 0.0;
        }
        let generator = TerrainGenerator::new(config);

        for chunk in [Chunk::new(0, 0), Chunk::new(-1, 0)] {
            let kind = generate(&generator, chunk);
            let origin: Vec3 = chunk.into();

            for x in 0..Chunk::X_AXIS_SIZE {
                for z in 0..Chunk::Z_AXIS_SIZE {
                    let height = generator.height(origin.x + x as f32, origin.z + z as f32);
                    let expected = (height.ceil() as i32 - 1) as u8;
                    let top = (0..=Chunk::Y_END)
                        .rev()
                        .find(|&y| !kind.get(ChunkVoxel::new(x as u8, y, z as u8)).is_none())
                        .unwrap_or_else(|| panic!("no solid voxel at ({x}, {z}) in {chunk:?}"));

                    assert_eq!(
                        top, expected,
                        "zero overhang moved the surface at ({x}, {z})"
                    );
                }
            }
        }
    }

    #[test]
    fn biome_terrain_report() {
        let generator = generator(42);
        let entries = &generator.config().biomes.entries;
        let mut heights = vec![Vec::new(); entries.len()];
        let mut slopes = vec![Vec::new(); entries.len()];
        let mut flat = vec![None::<(f32, f32, f32)>; entries.len()];
        let mut steep = vec![None::<(f32, f32, f32)>; entries.len()];
        let mut clamped_high = vec![0usize; entries.len()];
        let mut clamped_low = vec![0usize; entries.len()];
        let mut weights = vec![0.0; generator.biomes.len()];

        for x in 0..256 {
            for z in 0..256 {
                let world_x = (x * 16) as f32;
                let world_z = (z * 16) as f32;
                let index = biome_index(&generator, world_x, world_z);
                let height = generator.height(world_x, world_z);
                let slope = generator.slope(world_x, world_z);

                heights[index].push(height);
                slopes[index].push(slope);

                if flat[index].is_none_or(|(_, _, min)| slope < min) {
                    flat[index] = Some((world_x, world_z, slope));
                }
                if steep[index].is_none_or(|(_, _, max)| slope > max) {
                    steep[index] = Some((world_x, world_z, slope));
                }

                let erosion = terrain_erosion(&generator, world_x, world_z);

                generator.biomes.weights_into(
                    generator.temperature.sample(world_x, world_z),
                    generator.humidity.sample(world_x, world_z),
                    erosion,
                    &mut weights,
                );
                let shape = generator.biomes.blended_shape(&weights);
                let (base, amplitude, detail, peaks_and_valleys) =
                    independent_terrain(&generator, world_x, world_z);
                let raw = base
                    + peaks_and_valleys * amplitude * shape.relief_multiplier
                    + detail
                    + shape.height_offset;
                let terraced = raw
                    + shape.terrace_strength
                        * (terrace(raw, generator.config().biomes.terrace_step) - raw);
                let knee = generator.config().max_height as f32 - 16.0;

                if terraced > knee {
                    clamped_high[index] += 1;
                }
                if terraced < generator.config().min_height as f32 {
                    clamped_low[index] += 1;
                }
            }
        }

        let mut carved = vec![0usize; entries.len()];
        let mut solid = vec![0usize; entries.len()];
        let mut cave_column = vec![None::<(f32, f32, f32)>; entries.len()];

        // Sample chunks around each biome's representative flat column instead of a fixed
        // central window, so every biome gets cave statistics even when it is absent from the
        // spawn area.
        for (index, entry) in entries.iter().enumerate() {
            let Some((representative_x, representative_z, _)) = flat[index] else {
                println!("{}: no representative column for cave sampling", entry.name);
                continue;
            };
            let center = Chunk::new(
                (representative_x as i32).div_euclid(Chunk::X_AXIS_SIZE as i32),
                (representative_z as i32).div_euclid(Chunk::Z_AXIS_SIZE as i32),
            );

            for offset_x in -1..=1 {
                for offset_z in -1..=1 {
                    let chunk = Chunk::new(center.x + offset_x, center.z + offset_z);
                    let kind = generate(&generator, chunk);
                    let origin: Vec3 = chunk.into();

                    for x in 0..Chunk::X_AXIS_SIZE {
                        for z in 0..Chunk::Z_AXIS_SIZE {
                            let world_x = origin.x + x as f32;
                            let world_z = origin.z + z as f32;

                            if biome_index(&generator, world_x, world_z) != index {
                                continue;
                            }

                            let height = generator.height(world_x, world_z);
                            let top = (0..=Chunk::Y_END).rev().find(|&y| {
                                !kind.get(ChunkVoxel::new(x as u8, y, z as u8)).is_none()
                            });

                            if let Some(top) = top
                                && top as f32 + 8.0 < height
                                && cave_column[index].is_none()
                            {
                                cave_column[index] = Some((world_x, world_z, height));
                            }

                            let start = (height as usize).saturating_sub(8);
                            for y in 1..start {
                                if kind
                                    .get(ChunkVoxel::new(x as u8, y as u8, z as u8))
                                    .is_none()
                                {
                                    carved[index] += 1;
                                } else {
                                    solid[index] += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        for (index, entry) in entries.iter().enumerate() {
            let mut sorted_heights = heights[index].clone();
            let mut sorted_slopes = slopes[index].clone();
            sorted_heights.sort_by(f32::total_cmp);
            sorted_slopes.sort_by(f32::total_cmp);

            if sorted_heights.is_empty() {
                println!("{}: no sampled columns", entry.name);
                continue;
            }

            let percentile = |values: &[f32], fraction: f32| {
                values[((values.len() - 1) as f32 * fraction) as usize]
            };
            let sampled = carved[index] + solid[index];
            let carved_fraction = carved[index] as f32 / sampled.max(1) as f32;

            println!(
                "{}: columns={} height p10={:.1} p50={:.1} p90={:.1} slope p50={:.2} carved={:.1}% ({}/{} sampled) above_knee={:.1}% clamp_low={:.1}%",
                entry.name,
                sorted_heights.len(),
                percentile(&sorted_heights, 0.1),
                percentile(&sorted_heights, 0.5),
                percentile(&sorted_heights, 0.9),
                percentile(&sorted_slopes, 0.5),
                carved_fraction * 100.0,
                carved[index],
                sampled,
                clamped_high[index] as f32 / heights[index].len() as f32 * 100.0,
                clamped_low[index] as f32 / heights[index].len() as f32 * 100.0,
            );

            for (label, column) in [
                ("flat", flat[index]),
                ("steep", steep[index]),
                ("cave", cave_column[index]),
            ] {
                if let Some((world_x, world_z, value)) = column {
                    println!("  {label}: ({world_x}, {world_z}) value={value:.2}");
                }
            }
        }
    }

    fn independent_terrace(height: f32, step: f32) -> f32 {
        let cells = height / step;
        let whole = cells.floor();
        let fraction = cells - whole;
        let smooth = fraction * fraction * (3.0 - 2.0 * fraction);

        (whole + smooth) * step
    }

    fn uniform_terrace_config(strength: f32, step: f32) -> WorldGenConfig {
        let mut config = WorldGenConfig::load_default();
        config.biomes.terrace_step = step;

        for entry in &mut config.biomes.entries {
            entry.shape.height_offset = 0.0;
            entry.shape.relief_multiplier = 1.0;
            entry.shape.terrace_strength = strength;
            entry.shape.cave_density = 1.0;
            entry.shape.overhang_strength = config.caves.overhang_strength;
        }

        config
    }

    #[test]
    fn terrace_blend_matches_sample_column() {
        for (strength, step) in [(0.0, 8.0), (0.5, 8.0), (1.0, 8.0), (1.0, 12.0)] {
            let generator = TerrainGenerator::new(uniform_terrace_config(strength, step));

            for index in 0..64 {
                let x = (index % 8) as f32 * 23.0 - 96.0;
                let z = (index / 8) as f32 * 29.0 - 112.0;
                let (base, amplitude, detail, peaks_and_valleys) =
                    independent_terrain(&generator, x, z);
                let raw = base + peaks_and_valleys * amplitude + detail;
                let expected = soft_clamp_height(
                    raw + strength * (independent_terrace(raw, step) - raw),
                    generator.config().min_height as f32,
                    generator.config().max_height as f32,
                );
                let actual = generator.height(x, z);

                assert!(
                    (actual - expected).abs() < 1e-3,
                    "terrace blend differs at ({x}, {z}) with strength {strength}: {actual} != {expected}"
                );
            }
        }
    }

    #[test]
    fn terrace_strength_changes_the_height() {
        let flat = TerrainGenerator::new(uniform_terrace_config(0.0, 8.0));
        let full = TerrainGenerator::new(uniform_terrace_config(1.0, 8.0));
        let mut differs = false;

        for index in 0..64 {
            let x = (index % 8) as f32 * 23.0 - 96.0;
            let z = (index / 8) as f32 * 29.0 - 112.0;

            if (flat.height(x, z) - full.height(x, z)).abs() > 1e-3 {
                differs = true;
                break;
            }
        }

        assert!(differs, "full terrace strength never changed the height");
    }

    #[test]
    fn terrace_is_continuous_at_step_edges() {
        let step = 8.0;

        for cell in 0..20 {
            let edge = cell as f32 * step;
            let before = terrace(edge - 1e-3, step);
            let after = terrace(edge + 1e-3, step);

            assert!(
                (before - after).abs() < 1e-2,
                "terrace jumped at {edge}: {before} -> {after}"
            );
            assert!((before - edge).abs() < 1e-2);
        }

        for value in 0..=1000 {
            let height = value as f32 * 0.1;

            assert!(
                (terrace(height, step) - height).abs() <= 0.1 * step + 1e-3,
                "terrace deviation too large at {height}"
            );
        }
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
