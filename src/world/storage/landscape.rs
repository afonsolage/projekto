pub const HORIZONTAL_SIZE: usize = 8;
pub const VERTICAL_SIZE: usize = 1;

pub const HORIZONTAL_BEGIN: i32 = -(HORIZONTAL_SIZE as i32) / 2;
pub const HORIZONTAL_END: i32 = HORIZONTAL_BEGIN + HORIZONTAL_SIZE as i32;

pub const VERTICAL_BEGIN: i32 = -(VERTICAL_SIZE as i32) / 2;
pub const VERTICAL_END: i32 = VERTICAL_BEGIN + VERTICAL_SIZE as i32;

// pub fn is_within_bounds(local: IVec3) -> bool {
//     math::is_within_cubic_bounds(local, BEGIN, END)
// }
