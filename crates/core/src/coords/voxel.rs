use bevy::math::Vec3;

#[derive(Debug, Hash, PartialEq, Clone, Copy)]
pub struct Voxel {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Voxel {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

impl std::fmt::Display for Voxel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("({}, {}, {})", self.x, self.y, self.z))
    }
}

impl From<Vec3> for Voxel {
    fn from(world: Vec3) -> Self {
        // First round world coords to integer.
        // This transform (1.1, -0.3, 17.5) into (1, -1, 17)
        let x = world.x.floor() as i32;
        let y = world.y.floor() as i32;
        let z = world.z.floor() as i32;

        // Get the euclidean remainder
        // This transform (1, -1, 17) into (1, 15, 1)
        // let x = x.rem_euclid(Chunk::X_AXIS_SIZE as i32);
        // let y = y.rem_euclid(Chunk::Y_AXIS_SIZE as i32);
        // let z = z.rem_euclid(Chunk::Z_AXIS_SIZE as i32);

        Self { x, y, z }
    }
}

// #[cfg(test)]
// mod tests {
//     use rand::random;
//
//     use crate::coords::Chunk;
//
//     #[test]
//     fn to_world() {
//         use super::*;
//
//         const TEST_COUNT: usize = 1000;
//         const MAG: f32 = 100.0;
//
//         for _ in 0..TEST_COUNT {
//             let base_chunk = Chunk::new(
//                 (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
//                 (random::<f32>() * MAG) as i32 * if random::<bool>() { -1 } else { 1 },
//             );
//
//             let base_voxel = Voxel::new(
//                 (random::<f32>() * Chunk::X_AXIS_SIZE as f32) as i32,
//                 (random::<f32>() * Chunk::Y_AXIS_SIZE as f32) as i32,
//                 (random::<f32>() * Chunk::Z_AXIS_SIZE as f32) as i32,
//             );
//
//             let chunk_world = Vec3::new(base_chunk.x as f32, 0.0, base_chunk.z as f32)
//                 * Vec3::new(Chunk::X_AXIS_SIZE as f32, 0.0, Chunk::Z_AXIS_SIZE as f32);
//
//             assert_eq!(
//                 chunk_world + base_voxel.as_vec3(),
//                 super::to_world(base_voxel, base_chunk)
//             );
//         }
//     }
//
//     #[test]
//     fn to_local() {
//         assert_eq!(
//             IVec3::new(0, 0, 0),
//             super::to_local(Vec3::new(0.0, 0.0, 0.0))
//         );
//         assert_eq!(
//             IVec3::new(1, 0, 0),
//             super::to_local(Vec3::new(1.3, 0.0, 0.0))
//         );
//         assert_eq!(
//             IVec3::new(chunk::X_END, 0, 0),
//             super::to_local(Vec3::new(-0.3, 0.0, 0.0))
//         );
//         assert_eq!(
//             IVec3::new(chunk::X_END, 1, 0),
//             super::to_local(Vec3::new(-0.3, chunk::Y_AXIS_SIZE as f32 + 1.0, 0.0))
//         );
//         assert_eq!(
//             IVec3::new(1, chunk::Y_END, 1),
//             super::to_local(Vec3::new(1.1, -0.3, chunk::Z_AXIS_SIZE as f32 + 1.5))
//         );
//
//         const TEST_COUNT: usize = 1000;
//         const MAG: f32 = 100.0;
//
//         for _ in 0..TEST_COUNT {
//             // Generate a valid voxel number between 0 and chunk::AXIS_SIZE
//             let base = IVec3::new(
//                 (random::<f32>() * chunk::X_AXIS_SIZE as f32) as i32,
//                 (random::<f32>() * chunk::Y_AXIS_SIZE as f32) as i32,
//                 (random::<f32>() * chunk::Z_AXIS_SIZE as f32) as i32,
//             );
//
//             let sign = Vec3::new(
//                 if random::<bool>() { 1.0 } else { -1.0 },
//                 if random::<bool>() { 1.0 } else { -1.0 },
//                 if random::<bool>() { 1.0 } else { -1.0 },
//             );
//
//             // Generate some floating number between 0.0 and 0.9 just to simulate the fraction of
//             // world coordinates
//             let frag = Vec3::new(
//                 random::<f32>() * 0.9,
//                 random::<f32>() * 0.9,
//                 random::<f32>() * 0.9,
//             );
//
//             // Compute a valid world coordinates using the base voxel, the sign and the floating
//             // number
//             let world = Vec3::new(
//                 ((random::<f32>() * MAG * sign.x) as i32 * chunk::X_AXIS_SIZE as i32 + base.x)
//                     as f32,
//                 ((random::<f32>() * MAG * sign.y) as i32 * chunk::Y_AXIS_SIZE as i32 + base.y)
//                     as f32,
//                 ((random::<f32>() * MAG * sign.z) as i32 * chunk::X_AXIS_SIZE as i32 + base.z)
//                     as f32,
//             );
//
//             assert_eq!(
//                 base,
//                 super::to_local(world + frag),
//                 "Failed to convert {world:?} ({frag:?}) to local"
//             );
//         }
//     }
// }
