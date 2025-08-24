#[derive(Debug, Hash, PartialEq, Clone, Copy)]
pub struct Region {
    pub x: i32,
    pub z: i32,
}

impl Region {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}
