//! Deterministic hashing used to derive independent seeds and per-pixel noise.
//!
//! All hashing is integer based so the generated atlas is identical on every platform and
//! toolchain. `std`'s hashers are explicitly avoided because their output is not stable.

/// One round of the splitmix64 finalizer.
pub fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// Derives an independent seed from a base seed and a tag.
pub fn derive_seed(seed: u64, tag: u64) -> u64 {
    splitmix64(seed ^ tag.wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

/// Deterministic hash of two integer coordinates.
pub fn hash2d(x: i32, y: i32, seed: u64) -> u32 {
    let h = (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ seed;
    (splitmix64(h) >> 32) as u32
}

/// Deterministic hash in `[0, 1)`, using the top 24 bits so the `f32` conversion is exact.
pub fn hash2d_unit(x: i32, y: i32, seed: u64) -> f32 {
    let h = hash2d(x, y, seed);
    ((h >> 8) as f32) * (1.0 / 16_777_216.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable() {
        // Guards against accidental changes that would invalidate saved textures.
        assert_eq!(hash2d(3, 7, 42), 3_052_058_813);
        assert_eq!(hash2d(-3, 7, 42), 4_075_546_464);
    }

    #[test]
    fn unit_hash_stays_in_range() {
        for x in -32..32 {
            for y in -32..32 {
                let value = hash2d_unit(x, y, 42);
                assert!((0.0..1.0).contains(&value), "{value}");
            }
        }
    }

    #[test]
    fn derived_seeds_differ() {
        assert_ne!(derive_seed(42, 1), derive_seed(42, 2));
        assert_ne!(derive_seed(42, 1), derive_seed(43, 1));
    }
}
