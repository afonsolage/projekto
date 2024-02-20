use bevy::math::{IVec3, Vec3};

pub fn floor(vec: Vec3) -> IVec3 {
    IVec3::new(
        vec.x.floor() as i32,
        vec.y.floor() as i32,
        vec.z.floor() as i32,
    )
}

#[inline]
pub fn euclid_rem(vec: IVec3, div: IVec3) -> IVec3 {
    IVec3::new(
        vec.x.rem_euclid(div.x),
        vec.y.rem_euclid(div.y),
        vec.z.rem_euclid(div.z),
    )
}

#[derive(Debug, PartialEq, Eq, PartialOrd)]
pub enum Vec3Element {
    X,
    Y,
    Z,
}

pub fn abs_min_element(vec: Vec3) -> Vec3Element {
    let vec = vec.abs();

    if vec.x < vec.y && vec.x < vec.z {
        Vec3Element::X
    } else if vec.y < vec.x && vec.y < vec.z {
        Vec3Element::Y
    } else {
        Vec3Element::Z
    }
}

pub fn abs_max_element(vec: Vec3) -> Vec3Element {
    let vec = vec.abs();

    if vec.x > vec.y && vec.x > vec.z {
        Vec3Element::X
    } else if vec.y > vec.x && vec.y > vec.z {
        Vec3Element::Y
    } else {
        Vec3Element::Z
    }
}

pub fn pack(x: u8, y: u8, z: u8, w: u8) -> u32 {
    x as u32 | ((y as u32) << 8) | ((z as u32) << 16) | ((w as u32) << 24)
}

pub fn to_dir(world_dir: Vec3) -> IVec3 {
    match abs_max_element(world_dir) {
        Vec3Element::X => IVec3::X * world_dir.x.signum() as i32,
        Vec3Element::Y => IVec3::Y * world_dir.x.signum() as i32,
        Vec3Element::Z => IVec3::Z * world_dir.x.signum() as i32,
    }
}

#[inline]
pub fn to_unit_dir(dir: IVec3) -> Vec<IVec3> {
    let mut result = vec![];

    if dir.x == 1 {
        result.push((1, 0, 0).into());
    } else if dir.x == -1 {
        result.push((-1, 0, 0).into());
    }

    if dir.y == 1 {
        result.push((0, 1, 0).into());
    } else if dir.y == -1 {
        result.push((0, -1, 0).into());
    }

    if dir.z == 1 {
        result.push((0, 0, 1).into());
    } else if dir.z == -1 {
        result.push((0, 0, -1).into());
    }

    result
}

#[cfg(test)]
mod tests {

    #[test]
    fn pack() {
        let packed = super::pack(0, 0, 0, 0);
        assert_eq!(packed, 0);

        let packed = super::pack(1, 0, 0, 0);
        assert_eq!(packed, 1);

        let packed = super::pack(0, 1, 0, 0);
        assert_eq!(packed, 0xFF + 1);

        let packed = super::pack(0, 0, 1, 0);
        assert_eq!(packed, 0xFFFF + 1);

        let packed = super::pack(0, 0, 0, 1);
        assert_eq!(packed, 0xFFFFFF + 1);

        let packed = super::pack(1, 1, 1, 1);
        assert_eq!(packed, 0x01_01_01_01);

        let packed = super::pack(1, 2, 3, 4);
        assert_eq!(packed, 0x04_03_02_01);
    }

    #[test]
    fn floor() {
        let floor = super::floor((14.3, -1.1, -17.0).into());
        assert_eq!(floor, (14, -2, -17).into());
    }

    #[test]
    fn euclid_rem() {
        let rem = super::euclid_rem((16, -1, -17).into(), (15, 15, 15).into());
        assert_eq!(rem, (1, 14, 13).into());

        let rem = super::euclid_rem((14, 0, 0).into(), (15, 15, 8).into());
        assert_eq!(rem, (14, 0, 0).into());

        let rem = super::euclid_rem((-15, 32, 0).into(), (15, 30, 15).into());
        assert_eq!(rem, (0, 2, 0).into());
    }

    #[test]
    fn min_element() {
        let min = super::abs_min_element((-5.0, 4.0, 3.0).into());
        assert_eq!(min, super::Vec3Element::Z);

        let min = super::abs_min_element((5.0, -1.0, -3.0).into());
        assert_eq!(min, super::Vec3Element::Y);

        let min = super::abs_min_element((0.0, 0.0, 0.0).into());
        assert_eq!(min, super::Vec3Element::Z);
    }

    #[test]
    fn to_unit_dir() {
        let dirs = super::to_unit_dir((0, 0, 0).into());
        assert_eq!(dirs, vec![]);

        let dirs = super::to_unit_dir((1, 0, 0).into());
        assert_eq!(dirs, vec![(1, 0, 0).into()]);

        let dirs = super::to_unit_dir((1, 1, 0).into());
        assert_eq!(dirs, vec![(1, 0, 0).into(), (0, 1, 0).into()]);

        let dirs = super::to_unit_dir((1, 1, -1).into());
        assert_eq!(
            dirs,
            vec![(1, 0, 0).into(), (0, 1, 0).into(), (0, 0, -1).into()]
        );
    }
}
