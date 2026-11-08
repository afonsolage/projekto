//! Periodic value and cellular noise, sampled in tile-local `[0, 1)` space.
//!
//! Every field wraps at the tile border so generated tiles can repeat across greedy-meshed
//! quads. Lattice coordinates are wrapped with `rem_euclid`, which keeps the noise periodic at
//! every octave instead of only at the base frequency.

use crate::hash::{hash2d, hash2d_unit, splitmix64};

pub fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise on a square lattice that wraps every `cells` lattice points.
#[derive(Clone, Copy, Debug)]
pub struct PeriodicNoise {
    seed: u64,
    cells: i32,
}

impl PeriodicNoise {
    pub const fn new(seed: u64, cells: i32) -> Self {
        Self { seed, cells }
    }

    fn lattice(&self, x: i32, y: i32, cells: i32, octave: u32) -> f32 {
        let seed = self.seed ^ ((octave as u64) << 40);
        hash2d_unit(x.rem_euclid(cells), y.rem_euclid(cells), seed) * 2.0 - 1.0
    }

    /// Single-octave value noise in `[-1, 1]`.
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        self.octave(u, v, 0)
    }

    fn octave(&self, u: f32, v: f32, octave: u32) -> f32 {
        let cells = self.cells << octave;
        let fx = u * cells as f32;
        let fy = v * cells as f32;
        let x0 = fx.floor() as i32;
        let y0 = fy.floor() as i32;
        let tx = smoothstep(fx - x0 as f32);
        let ty = smoothstep(fy - y0 as f32);

        let top = lerp(
            self.lattice(x0, y0, cells, octave),
            self.lattice(x0 + 1, y0, cells, octave),
            tx,
        );
        let bottom = lerp(
            self.lattice(x0, y0 + 1, cells, octave),
            self.lattice(x0 + 1, y0 + 1, cells, octave),
            tx,
        );
        lerp(top, bottom, ty)
    }

    /// Fractal Brownian motion in roughly `[-1, 1]`.
    pub fn fbm(&self, u: f32, v: f32, octaves: u32) -> f32 {
        let mut sum = 0.0;
        let mut amplitude = 1.0;
        let mut total = 0.0;
        for octave in 0..octaves {
            sum += self.octave(u, v, octave) * amplitude;
            total += amplitude;
            amplitude *= 0.5;
        }
        sum / total.max(f32::EPSILON)
    }

    /// One-dimensional slice of the noise, useful for band heights and phase shifts.
    pub fn fbm_1d(&self, u: f32, octaves: u32) -> f32 {
        self.fbm(u, 0.317, octaves)
    }
}

/// A triangular-distributed value in `[0, 1)` from two lattice samples.
pub fn triangular(seed: u64, tag: u64, u: f32, v: f32) -> f32 {
    let noise = PeriodicNoise::new(splitmix64(seed ^ tag), 1);
    (noise.sample(u, v) + noise.sample(v, u)) * 0.5 + 0.5
}

#[derive(Clone, Copy, Debug)]
pub struct WorleySample {
    /// Distance to the nearest feature point, in cell units.
    pub f1: f32,
    /// Distance to the second nearest feature point, in cell units.
    pub f2: f32,
    /// Hash of the owning cell, stable per cell.
    pub cell: u32,
}

/// Jittered-grid Worley noise that wraps every `cells` cells.
#[derive(Clone, Copy, Debug)]
pub struct PeriodicWorley {
    seed: u64,
    cells: i32,
}

impl PeriodicWorley {
    pub const fn new(seed: u64, cells: i32) -> Self {
        Self { seed, cells }
    }

    pub fn sample(&self, u: f32, v: f32) -> WorleySample {
        let cells = self.cells;
        let fx = u * cells as f32;
        let fy = v * cells as f32;
        let base_x = fx.floor() as i32;
        let base_y = fy.floor() as i32;

        let mut f1 = f32::MAX;
        let mut f2 = f32::MAX;
        let mut cell = 0;

        for oy in -1..=1 {
            for ox in -1..=1 {
                let gx = base_x + ox;
                let gy = base_y + oy;
                let wx = gx.rem_euclid(cells);
                let wy = gy.rem_euclid(cells);
                let jitter = hash2d(wx, wy, self.seed);
                let jx = (jitter & 0xFF) as f32 / 255.0;
                let jy = ((jitter >> 8) & 0xFF) as f32 / 255.0;
                let dx = gx as f32 + jx - fx;
                let dy = gy as f32 + jy - fy;
                let distance = (dx * dx + dy * dy).sqrt();

                if distance < f1 {
                    f2 = f1;
                    f1 = distance;
                    cell = jitter;
                } else if distance < f2 {
                    f2 = distance;
                }
            }
        }

        WorleySample { f1, f2, cell }
    }
}

/// Linear interpolation.
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_periodic() {
        let noise = PeriodicNoise::new(7, 4);
        for v in [0.0, 0.25, 0.5, 0.75] {
            let left = noise.fbm(0.0, v, 3);
            let right = noise.fbm(1.0, v, 3);
            assert!((left - right).abs() < 1e-4, "{left} vs {right}");
        }
    }

    #[test]
    fn worley_is_periodic() {
        let worley = PeriodicWorley::new(9, 3);
        let left = worley.sample(0.0, 0.5);
        let right = worley.sample(1.0, 0.5);
        assert!((left.f1 - right.f1).abs() < 1e-4);
        assert!((left.f2 - right.f2).abs() < 1e-4);
    }

    #[test]
    fn fbm_stays_in_range() {
        let noise = PeriodicNoise::new(11, 3);
        for x in 0..32 {
            for y in 0..32 {
                let value = noise.fbm(x as f32 / 32.0, y as f32 / 32.0, 4);
                assert!(value.abs() <= 1.0, "{value}");
            }
        }
    }
}
