use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::OnceLock,
};

use bevy::prelude::*;
use projekto_core::{
    chunk::{Chunk, ChunkStorage},
    voxel,
};
use serde::{Deserialize, Serialize};

const CACHE_DIR: &str = "world/chunks/";
const CACHE_EXT: &str = "bin";

static CACHE_PATH: OnceLock<PathBuf> = OnceLock::new();

#[derive(Default, Serialize, Deserialize)]
pub struct ChunkCache {
    pub chunk: Chunk,
    pub kind: ChunkStorage<voxel::Kind>,
    pub light: ChunkStorage<voxel::Light>,
    pub occlusion: ChunkStorage<voxel::FacesOcclusion>,
    pub soft_light: ChunkStorage<voxel::FacesSoftLight>,
    pub vertex: Vec<voxel::Vertex>,
}

impl ChunkCache {
    pub fn init(root: &str) -> bool {
        let new_path = init_path(root);
        if let Err(existing) = CACHE_PATH.set(new_path.clone()) {
            if new_path != existing {
                warn!("Failed to init CachePath. Another thread already initialized it to {new_path:?}");
                return false;
            }
        }

        true
    }

    pub fn load(chunk: Chunk) -> Option<Self> {
        let path = Self::path(chunk);
        let file = std::fs::OpenOptions::new().read(true).open(path);

        let Ok(mut file) = file else {
            let error = file.expect_err("Is an error");
            error!("Failed to load chunk {chunk:?}. Error: {error}");
            return None;
        };

        let mut compressed = vec![];
        if let Err(error) = file.read_to_end(&mut compressed) {
            error!("Failed to read chunk {chunk:?} from disk. Error: {error}");
            return None;
        }

        let decompressed = lz4_flex::decompress_size_prepended(&compressed);
        let Ok(decompressed) = decompressed else {
            let error = decompressed.expect_err("Is an error");
            error!("Failed to decompress chunk {chunk:?}. Error: {error}");
            return None;
        };

        match bincode::deserialize(&decompressed) {
            Ok(cache) => Some(cache),
            Err(error) => {
                error!("Failed to deserialize chunk {chunk:?}. Error: {error}");
                None
            }
        }
    }

    pub fn save(self) -> bool {
        let path = Self::path(self.chunk);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path);

        let Ok(mut file) = file else {
            let error = file.expect_err("Is an error");
            let chunk = self.chunk;
            error!("Failed to save chunk {chunk:?}. Error: {error}");
            return false;
        };

        let result = bincode::serialize(&self);
        let Ok(bytes) = result else {
            let error = result.expect_err("Is an error");
            let chunk = self.chunk;
            error!("Failed to serialize chunk {chunk:?}. Error: {error}");
            return false;
        };

        let compressed = lz4_flex::compress_prepend_size(&bytes);
        if let Err(error) = file.write_all(&compressed) {
            let chunk = self.chunk;
            error!("Failed to write chunk {chunk:?} on disk. Error: {error}");
            return false;
        }

        true
    }

    pub fn delete(chunk: Chunk) {
        let path = Self::path(chunk);
        let _ = std::fs::remove_file(path);
    }

    pub fn file_name(chunk: Chunk) -> String {
        chunk
            .xz()
            .to_string()
            .chars()
            .filter_map(|c| match c {
                ',' => Some('_'),
                ' ' | '[' | ']' => None,
                _ => Some(c),
            })
            .collect()
    }

    pub fn path(chunk: Chunk) -> PathBuf {
        PathBuf::from(CACHE_PATH.get_or_init(|| init_path(std::env::temp_dir().to_str().unwrap())))
            .with_file_name(Self::file_name(chunk))
            .with_extension(CACHE_EXT)
    }
}

fn init_path(root: &str) -> PathBuf {
    let path = PathBuf::from(root).join(CACHE_DIR);

    if !path.exists() {
        if let Err(err) = std::fs::create_dir_all(&path) {
            error!("Failed to create cache folder at {path:?}. Error: {err}");
        } else {
            info!("Created cache folder at {path:?}");
        }
    }

    path
}

#[cfg(test)]
mod tests {
    use projekto_core::chunk::Chunk;

    use crate::cache::ChunkCache;

    #[test]
    fn file_name() {
        assert_eq!("-234_1", ChunkCache::file_name(Chunk::new(-234, 1)));
        assert_eq!(
            "-9999_-9999",
            ChunkCache::file_name(Chunk::new(-9999, -9999))
        );
        assert_eq!("9999_-9999", ChunkCache::file_name(Chunk::new(9999, -9999)));
        assert_eq!("0_0", ChunkCache::file_name(Chunk::new(0, 0)));
    }

    #[test]
    fn save() {
        let _ = tracing_subscriber::fmt().try_init();

        let cache = ChunkCache::default();
        let path = ChunkCache::path(cache.chunk);
        let _ = std::fs::remove_file(path);

        let saved = cache.save();

        assert!(saved, "Cache must be saved");

        assert!(
            ChunkCache::path(Chunk::default()).exists(),
            "File must be created"
        );

        ChunkCache::delete(Chunk::default());
    }
}
