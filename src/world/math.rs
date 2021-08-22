use bevy::prelude::*;


pub fn is_within_cubic_bounds(pos: &IVec3, min: i32, max: i32) -> bool {
    pos.min_element() >= min && pos.max_element() <= max
}

pub fn to_grid_dir(dir: &Vec3) -> IVec3 {
    IVec3::new(
        if dir.x >= 0.0 { 1 } else { -1 },
        if dir.y >= 0.0 { 1 } else { -1 },
        if dir.z >= 0.0 { 1 } else { -1 },
    )
}

pub fn trunc(vec: &Vec3) -> IVec3 {
    IVec3::new(
        vec.x.trunc() as i32,
        vec.y.trunc() as i32,
        vec.z.trunc() as i32,
    )
}

// pub fn get_min_abs_axis(vec: Vec3) -> f32 {
//     let abs = vec.abs();
//     if abs.x < abs.y && abs.x < abs.z {
//         vec.x
//     } else if abs.y < abs.x && abs.y < abs.z {
//         vec.y
//     } else {
//         vec.z
//     }
// }

// pub fn to_unit_axis_ivec3(vec: Vec3) -> IVec3 {
//     let abs = vec.normalize().abs();
//     if abs.x > abs.y && abs.x > abs.z {
//         (vec.x.signum() as i32) * IVec3::X
//     } else if abs.y > abs.x && abs.y > abs.z {
//         (vec.y.signum() as i32) * IVec3::Y
//     } else {
//         (vec.z.signum() as i32) * IVec3::Z
//     }
// }