use std::path::PathBuf;

use super::chunk::ChunkKind;

use bevy::math::IVec3;
use serde::{Deserialize, Serialize};

const AXIS_SIZE: usize = 16;
const BUFFER_SIZE: usize = AXIS_SIZE * AXIS_SIZE * AXIS_SIZE;

const CACHE_PATH: &str = "cache/sectors/example";
const CACHE_EXT: &str = "bin";

const X_MASK: usize = 0b_1111_0000_0000;
const Z_MASK: usize = 0b_0000_1111_0000;
const Y_MASK: usize = 0b_0000_0000_1111;

const X_SHIFT: usize = 8;
const Z_SHIFT: usize = 4;
const Y_SHIFT: usize = 0;

#[derive(Debug, Serialize, Deserialize)]
struct Sector {
    local: IVec3,
    kinds: Vec<ChunkKind>,
}

impl Sector {
    fn new(local: IVec3) -> Self {
        Sector {
            local,
            kinds: vec![ChunkKind::default(); BUFFER_SIZE],
        }
    }

    fn load(local: IVec3) -> Self {
        perf_fn_scope!();

        let path = local_path(local);

        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .unwrap_or_else(|_| panic!("Unable to open file {}", path.display()));

        let buffer = std::io::BufReader::new(file);

        bincode::deserialize_from(buffer)
            .unwrap_or_else(|_| panic!("Failed to parse file {}", path.display()))
    }

    fn save(&self) {
        perf_fn_scope!();

        let path = local_path(self.local);

        let file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&path)
            .unwrap_or_else(|_| panic!("Unable to write to file {}", path.display()));

        let buffer = std::io::BufWriter::new(file);

        bincode::serialize_into(buffer, self)
            .unwrap_or_else(|_| panic!("Failed to serialize sector to file {}", path.display()));
    }

    pub fn get(&self, local: IVec3) -> &ChunkKind {
        &self.kinds[to_index(local)]
    }

    pub fn get_mut(&mut self, local: IVec3) -> &mut ChunkKind {
        &mut self.kinds[to_index(local)]
    }
}

#[cfg(test)]
impl PartialEq for Sector {
    fn eq(&self, other: &Self) -> bool {
        self.local == other.local && self.kinds == other.kinds
    }
}

pub fn to_index(local: IVec3) -> usize {
    (local.x << X_SHIFT | local.y << Y_SHIFT | local.z << Z_SHIFT) as usize
}

// fn from_index(index: usize) -> IVec3 {
//     IVec3::new(
//         ((index & X_MASK) >> X_SHIFT) as i32,
//         ((index & Y_MASK) >> Y_SHIFT) as i32,
//         ((index & Z_MASK) >> Z_SHIFT) as i32,
//     )
// }

fn local_path(local: IVec3) -> PathBuf {
    PathBuf::from(CACHE_PATH)
        .with_file_name(format_local(local))
        .with_extension(CACHE_EXT)
}

fn format_local(local: IVec3) -> String {
    local
        .to_string()
        .chars()
        .filter_map(|c| match c {
            ',' => Some('_'),
            ' ' | '[' | ']' => None,
            _ => Some(c),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn create_sector(path: &Path, sector: &Sector) {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();

        let buf_writer = std::io::BufWriter::new(file);

        bincode::serialize_into(buf_writer, sector).unwrap();
    }

    #[test]
    fn local_path() {
        let path = super::local_path((0, 0, 0).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("0_0_0.{}", CACHE_EXT)));

        let path = super::local_path((-1, 0, 0).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("-1_0_0.{}", CACHE_EXT)));

        let path = super::local_path((-1, 3333, -461).into())
            .to_str()
            .unwrap()
            .to_string();

        assert!(path.ends_with(&format!("-1_3333_-461.{}", CACHE_EXT)));
    }

    #[test]
    fn to_index() {
        assert_eq!(super::to_index((0, 0, 0).into()), 0);
        assert_eq!(super::to_index((0, 1, 0).into()), 1);
        assert_eq!(super::to_index((0, 2, 0).into()), 2);

        assert_eq!(super::to_index((0, 0, 1).into()), super::AXIS_SIZE);
        assert_eq!(super::to_index((0, 1, 1).into()), super::AXIS_SIZE + 1);
        assert_eq!(super::to_index((0, 2, 1).into()), super::AXIS_SIZE + 2);

        assert_eq!(
            super::to_index((1, 0, 0).into()),
            super::AXIS_SIZE * super::AXIS_SIZE
        );
        assert_eq!(
            super::to_index((1, 1, 0).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + 1
        );
        assert_eq!(
            super::to_index((1, 2, 0).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + 2
        );

        assert_eq!(
            super::to_index((1, 0, 1).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE
        );
        assert_eq!(
            super::to_index((1, 1, 1).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE + 1
        );
        assert_eq!(
            super::to_index((1, 2, 1).into()),
            super::AXIS_SIZE * super::AXIS_SIZE + super::AXIS_SIZE + 2
        );
    }

    #[test]
    fn format_local() {
        assert_eq!("-234_22_1", super::format_local((-234, 22, 1).into()));
        assert_eq!(
            "-9999_-9999_-9999",
            super::format_local((-9999, -9999, -9999).into())
        );
        assert_eq!(
            "9999_-9999_9999",
            super::format_local((9999, -9999, 9999).into())
        );
        assert_eq!("0_0_0", super::format_local((0, 0, 0).into()));
    }

    #[test]
    fn ser_de() {
        let mut temp_file = std::env::temp_dir();
        temp_file.push("test.tmp");

        let sector = Sector::new((0, 0, 0).into());

        create_sector(&temp_file, &sector);

        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(&temp_file)
            .unwrap();

        let buf_reader = std::io::BufReader::new(file);

        let sector_loaded: Sector = bincode::deserialize_from(buf_reader).unwrap();

        assert_eq!(sector, sector_loaded);
    }

    #[test]
    fn save() {
        let sector = Sector::new((-1, 2, 3).into());

        sector.save();

        let path = super::local_path(sector.local);
        assert!(path.exists());

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load() {
        let local = (-3, -2, 5).into();
        let mut sector = Sector::new(local);

        let chunk = sector.get_mut((2, 3, 5).into());
        chunk.set((5, 4, 3).into(), 15.into());

        sector.save();

        let loaded = Sector::load(local);

        assert_eq!(sector, loaded);

        let loaded_chunk = loaded.get((2, 3, 5).into());
        assert_eq!(loaded_chunk.get((5, 4, 3).into()), 15.into());

        let path = super::local_path(sector.local);
        std::fs::remove_file(path).unwrap();
    }
}
