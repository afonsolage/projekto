use bevy::prelude::*;

pub fn is_within_cubic_bounds(pos: IVec3, min: i32, max: i32) -> bool {
    pos.min_element() >= min && pos.max_element() <= max
}

pub fn floor(vec: Vec3) -> IVec3 {
    IVec3::new(
        vec.x.floor() as i32,
        vec.y.floor() as i32,
        vec.z.floor() as i32,
    )
}

pub fn euclid_rem(vec: IVec3, div: i32) -> IVec3 {
    IVec3::new(
        vec.x.rem_euclid(div),
        vec.y.rem_euclid(div),
        vec.z.rem_euclid(div),
    )
}

#[derive(Debug, PartialEq, PartialOrd)]
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
    fn is_within_cubic_bounds() {
        assert!(super::is_within_cubic_bounds((1, 2, 3).into(), 0, 15));
        assert!(!super::is_within_cubic_bounds((-1, 2, 3).into(), 0, 15));
        assert!(super::is_within_cubic_bounds((0, 0, 0).into(), 0, 15));
        assert!(super::is_within_cubic_bounds((15, 15, 15).into(), 0, 15));
        assert!(!super::is_within_cubic_bounds((15, 16, 15).into(), 0, 15));
    }

    #[test]
    fn floor() {
        let floor = super::floor((14.3, -1.1, -17.0).into());
        assert_eq!(floor, (14, -2, -17).into());
    }

    #[test]
    fn euclid_rem() {
        let rem = super::euclid_rem((16, -1, -17).into(), 15);
        assert_eq!(rem, (1, 14, 13).into());

        let rem = super::euclid_rem((14, 0, 0).into(), 15);
        assert_eq!(rem, (14, 0, 0).into());

        let rem = super::euclid_rem((-15, 16, 0).into(), 15);
        assert_eq!(rem, (0, 1, 0).into());
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
