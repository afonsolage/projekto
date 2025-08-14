#![allow(unused)]

use async_fs::{File, OpenOptions};
use bevy::{math::IVec2, prelude::*};
use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use projekto_core::chunk::Chunk;
use std::{
    cell::{OnceCell, RefCell},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
};
use thiserror::Error;

mod server;
pub use server::ArchiveServer;

const SECTOR_SIZE: usize = 4096;
const AXIS_CHUNK_COUNT: usize = 32;
const MAX_CHUNK_COUNT: usize = AXIS_CHUNK_COUNT * AXIS_CHUNK_COUNT;
const SECTOR_INDEX_SIZE: usize = 2 * 2; // 2 bytes, one for offset and another for sectors
const HEADER_SIZE: usize = SECTOR_INDEX_SIZE * MAX_CHUNK_COUNT;
const AXIS_SHIFT: usize = AXIS_CHUNK_COUNT.ilog2() as usize;

pub(crate) fn chunk_local_to_archive(chunk: Chunk) -> IVec2 {
    IVec2::new(
        ((chunk.x() as f32) / AXIS_CHUNK_COUNT as f32).floor() as i32,
        ((chunk.z() as f32) / AXIS_CHUNK_COUNT as f32).floor() as i32,
    )
}

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Failed to decode: {0}")]
    Decode(#[from] bincode::error::DecodeError),
    #[error("Failed to encode: {0}")]
    Encode(#[from] bincode::error::EncodeError),
    #[error("Failed to compress: {0}")]
    Compress(#[from] lz4_flex::frame::Error),
    #[error("Invalid header size: {0}")]
    HeaderInvalid(usize),
    #[error("Failed to write: {0}")]
    Write(String),
    #[error("Failed to load chunk: {0}")]
    ChunkLoad(String),
    #[error("Failed to save chunk: {0}")]
    ChunkSave(String),
    #[error("Failed to receive task result")]
    TaskRecv,
}

#[derive(Default, Debug, Clone, Copy)]
struct SectorIndex {
    offset: u16,
    sectors: u16,
}

impl SectorIndex {
    fn is_empty(&self) -> bool {
        // A valid offset will always be after header, so non-zero valid.
        self.offset == 0
    }

    fn seek_offset(&self) -> u64 {
        assert_ne!(self.offset, 0);

        self.offset as u64 * SECTOR_SIZE as u64
    }

    fn bytes_count(&self) -> usize {
        Self::sector_to_bytes(self.sectors)
    }

    fn sectors_count(bytes: usize) -> u16 {
        ((bytes + SECTOR_SIZE + 1) / SECTOR_SIZE) as u16
    }

    fn from_seek_position(seek_position: u64, needed_sectors: u16) -> SectorIndex {
        assert_eq!(
            (seek_position + HEADER_SIZE as u64) % SECTOR_SIZE as u64,
            0u64,
            "Invalid seek position: {seek_position}. It should be in blocks of {SECTOR_SIZE}"
        );

        let offset = (seek_position / SECTOR_SIZE as u64) as u16;

        Self {
            offset,
            sectors: needed_sectors,
        }
    }

    fn as_bytes(&self) -> [u8; 4] {
        ((self.offset as u32) << 16 | self.sectors as u32).to_be_bytes()
    }

    fn from_bytes(bytes: [u8; 4]) -> Self {
        let i = u32::from_be_bytes(bytes);
        Self {
            offset: (i >> 16) as u16,
            sectors: (i & 0xFFFF) as u16,
        }
    }

    fn sector_to_bytes(sectors: u16) -> usize {
        sectors as usize * SECTOR_SIZE
    }
}

#[derive(Default)]
struct Header {
    sectors: Vec<SectorIndex>,
    dirty: bool,
}

impl Header {
    fn new() -> Self {
        Self {
            sectors: vec![Default::default(); MAX_CHUNK_COUNT],
            dirty: false,
        }
    }

    fn de(bytes: &[u8]) -> Result<Self, ArchiveError> {
        if bytes.len() != HEADER_SIZE {
            return Err(ArchiveError::HeaderInvalid(bytes.len()));
        }

        let sectors = bytes
            .as_chunks()
            .0 // Discard remainder
            .iter()
            .copied()
            .map(SectorIndex::from_bytes)
            .collect::<Vec<_>>();

        Ok(Self {
            sectors,
            dirty: false,
        })
    }

    fn ser(&self) -> Result<Vec<u8>, ArchiveError> {
        let bytes = self
            .sectors
            .iter()
            .flat_map(SectorIndex::as_bytes)
            .collect::<Vec<_>>();

        if bytes.len() != HEADER_SIZE {
            Err(ArchiveError::HeaderInvalid(bytes.len()))
        } else {
            Ok(bytes)
        }
    }

    fn get_index(&self, x: u8, z: u8) -> SectorIndex {
        self.sectors[Self::to_index(x, z)]
    }

    fn set_index(&mut self, x: u8, z: u8, index: SectorIndex) {
        self.sectors[Self::to_index(x, z)] = index;
        self.dirty = true;
    }

    #[inline]
    fn to_index(x: u8, z: u8) -> usize {
        (x as usize) << AXIS_SHIFT | z as usize
    }
}

pub struct Archive<T> {
    header: Header,
    file_handler: File,
    _pd: std::marker::PhantomData<T>,
}

impl<T> Archive<T> {
    pub async fn new(name: &str) -> Result<Self, ArchiveError> {
        let path = std::path::Path::new(name);

        if let Some(parent_dir) = path.parent()
            && !std::fs::exists(parent_dir)?
        {
            std::fs::create_dir_all(parent_dir)?;
        }

        let mut file_handler = OpenOptions::new()
            .read(true)
            .write(true)
            .truncate(false)
            .create(true)
            .open(std::path::Path::new(name))
            .await?;

        let file_len = file_handler.seek(SeekFrom::End(0)).await?;

        let header = if file_len > 0 {
            let mut bytes = vec![0u8; HEADER_SIZE];
            file_handler.seek(SeekFrom::Start(0)).await?;
            file_handler.read_exact(&mut bytes).await?;
            Header::de(&bytes)?
        } else {
            let mut header = Header::new();
            header.dirty = true;
            header
        };

        let mut archive = Self {
            header,
            file_handler,
            _pd: Default::default(),
        };

        if archive.is_header_dirty() {
            archive.save_header().await?;
        }

        Ok(archive)
    }

    pub async fn save_header(&mut self) -> Result<(), ArchiveError> {
        let bytes = self.header.ser()?;
        self.file_handler.seek(SeekFrom::Start(0)).await?;
        self.file_handler.write_all(&bytes).await?;
        self.file_handler.flush().await?;
        self.header.dirty = false;

        Ok(())
    }

    pub fn is_header_dirty(&self) -> bool {
        self.header.dirty
    }
}

impl<T> Archive<T>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    pub async fn read(&mut self, x: u8, z: u8) -> Result<Option<T>, ArchiveError> {
        let index = self.header.get_index(x, z);

        if index.is_empty() {
            return Ok(None);
        }

        let mut buffer = vec![0u8; index.bytes_count()];

        self.file_handler
            .seek(SeekFrom::Start(index.seek_offset()))
            .await?;

        self.file_handler.read_exact(&mut buffer).await?;

        let mut frame = lz4_flex::frame::FrameDecoder::new(&*buffer);
        let value: T =
            bincode::serde::decode_from_std_read(&mut frame, bincode::config::standard())?;

        Ok(Some(value))
    }

    pub async fn write(&mut self, x: u8, z: u8, value: T) -> Result<(), ArchiveError> {
        let mut compressed = Vec::with_capacity(256 * 1024); // 256k
        let mut frame = lz4_flex::frame::FrameEncoder::new(&mut compressed);
        bincode::serde::encode_into_std_write(&value, &mut frame, bincode::config::standard())?;
        frame.finish()?;

        let index = self.header.get_index(x, z);
        let needed_sectors = SectorIndex::sectors_count(compressed.len());

        // Check sector count
        let index = if index.is_empty() || needed_sectors > index.sectors {
            self.append(SectorIndex::sectors_count(compressed.len()))
                .await?
        } else {
            index
        };

        if index.is_empty() {
            return Err(ArchiveError::Write(format!(
                "Unable to find a index at {x}, {z}"
            )));
        }

        let new_len = SectorIndex::sector_to_bytes(needed_sectors);
        compressed.resize(new_len, 0);

        self.header.set_index(x, z, index);
        self.file_handler
            .seek(SeekFrom::Start(index.seek_offset()))
            .await?;
        self.file_handler.write_all(&compressed).await?;
        self.file_handler.flush().await?;

        Ok(())
    }

    async fn append(&mut self, needed_sectors: u16) -> Result<SectorIndex, ArchiveError> {
        let seek_position = self.file_handler.seek(SeekFrom::End(0)).await?;

        Ok(SectorIndex::from_seek_position(
            seek_position,
            needed_sectors,
        ))
    }
}

#[cfg(test)]
mod tests {
    use bevy::tasks::block_on;
    use projekto_core::chunk::{self, ChunkStorage};
    use rand::{Rng, SeedableRng, rngs::StdRng};

    use super::*;

    fn temp_file() -> String {
        format!(
            "{}/projekto/{:#08}.tmp",
            std::env::temp_dir().display(),
            rand::random_range(0..u32::MAX)
        )
    }

    #[test]
    fn sector_index_as_bytes() {
        // Arrange
        let index = SectorIndex {
            offset: 123,
            sectors: 9821,
        };

        // Act
        let bytes = index.as_bytes();

        // Assert
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x7B);
        assert_eq!(bytes[2], 0x26);
        assert_eq!(bytes[3], 0x5D);
    }

    #[test]
    fn sector_index_from_bytes() {
        // Arrange
        let bytes = [0x00, 0x37, 0x03, 0xE7];

        // Act
        let index = SectorIndex::from_bytes(bytes);

        // Assert
        assert_eq!(index.offset, 55);
        assert_eq!(index.sectors, 999);
    }

    #[test]
    fn header_get_set() {
        // Arrange
        let mut header = Header::new();

        // Act
        for x in 0..16 {
            for z in 0..16 {
                header.set_index(
                    x,
                    z,
                    SectorIndex {
                        offset: x as u16,
                        sectors: z as u16,
                    },
                );
            }
        }

        // Assert
        for x in 0..16 {
            for z in 0..16 {
                let index = header.get_index(x, z);

                assert_eq!(index.offset, x as u16);
                assert_eq!(index.sectors, z as u16);
            }
        }
        assert!(header.dirty);
    }

    #[test]
    fn header_ser_de() {
        // Arrange
        let mut header = Header::new();
        header
            .sectors
            .iter_mut()
            .enumerate()
            .for_each(|(i, index)| {
                index.offset = i as u16;
                index.sectors = ((i + 10) / 15) as u16;
            });

        // Act
        let buffer = header.ser().expect("Serialize to work fine");
        let header = Header::de(&buffer).expect("Deserialize to work fine");

        // Assert
        header.sectors.iter().enumerate().for_each(|(i, index)| {
            assert_eq!(index.offset, i as u16);
            assert_eq!(index.sectors, ((i + 10) / 15) as u16);
        });
        assert!(!header.dirty);
    }

    #[test]
    fn archive_new() {
        // Arrange
        let temp_file = temp_file();

        // Act
        let res = block_on(Archive::<u8>::new(&temp_file)).unwrap();

        // Assert
        assert!(std::fs::exists(&temp_file).unwrap());

        let metadata = std::fs::metadata(&temp_file).unwrap();

        assert_eq!(metadata.len(), HEADER_SIZE as u64);
    }

    #[test]
    fn archive_new_existing_file_empty() {
        // Arrange
        let temp_file = temp_file();
        let file = block_on(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&temp_file),
        )
        .unwrap();
        drop(file);

        // Act
        let _res = block_on(Archive::<u8>::new(&temp_file)).unwrap();

        // Assert
    }

    #[test]
    fn archive_new_existing_header_invalid() {
        // Arrange
        let temp_file = temp_file();
        std::fs::write(&temp_file, [0x00, 0xFF, 0xF1, 0x12]).unwrap();

        // Act
        let res = block_on(Archive::<u8>::new(&temp_file));

        // Assert
        assert!(matches!(res, Err(ArchiveError::IO(_))));
    }

    #[test]
    fn archive_new_existing_header_valid() {
        // Arrange
        let temp_file = temp_file();

        let mut header = Header::new();
        header
            .sectors
            .iter_mut()
            .enumerate()
            .for_each(|(i, index)| {
                index.offset = i as u16;
                index.sectors = ((i + 10) / 15) as u16;
            });

        std::fs::write(&temp_file, header.ser().unwrap()).unwrap();

        // Act
        let archive = block_on(Archive::<u8>::new(&temp_file)).unwrap();

        // Assert
        archive
            .header
            .sectors
            .iter()
            .enumerate()
            .for_each(|(i, index)| {
                assert_eq!(index.offset, i as u16);
                assert_eq!(index.sectors, ((i + 10) / 15) as u16);
            });
        assert!(!archive.header.dirty);
    }

    #[test]
    fn archive_header_save_load() {
        // Arrange
        let temp_file = temp_file();

        let mut archive = block_on(Archive::<u8>::new(&temp_file)).unwrap();
        archive
            .header
            .sectors
            .iter_mut()
            .enumerate()
            .for_each(|(i, index)| {
                index.offset = (MAX_CHUNK_COUNT - i) as u16;
                index.sectors = i as u16;
            });

        // Act
        block_on(archive.save_header()).unwrap();
        drop(archive);
        let archive = block_on(Archive::<u8>::new(&temp_file)).unwrap();

        // Assert
        archive
            .header
            .sectors
            .iter()
            .enumerate()
            .for_each(|(i, index)| {
                assert_eq!(index.offset, (MAX_CHUNK_COUNT - i) as u16);
                assert_eq!(index.sectors, i as u16);
            });
        assert!(!archive.is_header_dirty());
    }

    #[test]
    fn archive_read_write_single() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = block_on(Archive::<String>::new(&temp_file)).unwrap();
        let txt = "The Silly Goosery is real!🪿︎";

        // Act
        block_on(archive.write(2, 3, txt.to_string())).unwrap();
        let read_txt = block_on(archive.read(2, 3)).unwrap();

        // Assert
        let read_txt = read_txt.unwrap();
        assert_eq!(read_txt, txt);
    }

    #[test]
    fn archive_write_same_sector_count() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = block_on(Archive::<Vec<u16>>::new(&temp_file)).unwrap();
        block_on(archive.write(2, 3, ((0..20u16).collect()))).unwrap();

        // Act
        let new_value = (0..1000u16).collect::<Vec<_>>();
        block_on(archive.write(2, 3, new_value.clone())).unwrap();

        // Assert
        let read_value = block_on(archive.read(2, 3)).unwrap();
        let read_value = read_value.unwrap();
        assert_eq!(new_value, read_value);

        let index = archive.header.get_index(2, 3);
        assert_eq!(index.offset, 1);
        assert_eq!(index.sectors, 1);
    }

    #[test]
    fn archive_write_append_sector() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = block_on(Archive::<Vec<u16>>::new(&temp_file)).unwrap();
        block_on(archive.write(2, 3, ((0..20u16).collect()))).unwrap();
        let old_index = archive.header.get_index(2, 3);

        // Act
        let new_value = (0..4000u16).collect::<Vec<_>>();
        block_on(archive.write(2, 3, new_value.clone())).unwrap();

        // Assert
        let read_value = block_on(archive.read(2, 3)).unwrap();
        let read_value = read_value.unwrap();
        assert_eq!(new_value, read_value);

        let new_index = archive.header.get_index(2, 3);
        assert_ne!(old_index.offset, new_index.offset);
        assert_ne!(old_index.sectors, new_index.sectors);
    }

    fn generate_chunk(seed: u64) -> ChunkStorage<u128> {
        let mut rnd = StdRng::seed_from_u64(seed);
        let mut chunk = ChunkStorage::<u128>::default();
        chunk::voxels().for_each(|voxel| {
            chunk.set(voxel, rnd.random());
        });

        chunk
    }

    #[test]
    fn archive_many() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = block_on(Archive::new(&temp_file)).unwrap();
        let mut chunks = vec![];

        // Act
        for x in 0..3u8 {
            let chunk = generate_chunk(x as u64);
            chunks.push((x, chunk.clone()));
            block_on(archive.write(x, 0, chunk)).unwrap();
        }

        // Assert
        for (x, chunk) in chunks {
            let chunk = generate_chunk(x as u64);
            let cached_chunk = block_on(archive.read(x, 0));

            if cached_chunk.is_err() {
                panic!("Failed at {x}, 0. Error: {cached_chunk:?}");
            }

            let cached_chunk = cached_chunk.unwrap().unwrap();

            assert_eq!(chunk, cached_chunk);
        }
    }

    #[test]
    fn archive_max_indices() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = block_on(Archive::new(&temp_file)).unwrap();

        // Act
        for x in 30..AXIS_CHUNK_COUNT as u8 {
            for z in 0..AXIS_CHUNK_COUNT as u8 {
                let value = (x as u128) << 8 | z as u128;
                block_on(archive.write(x, z, value)).unwrap();
            }
        }

        // Assert
        for x in 30..AXIS_CHUNK_COUNT as u8 {
            for z in 0..AXIS_CHUNK_COUNT as u8 {
                let value = block_on(archive.read(x, z));

                if value.is_err() {
                    panic!("Failed at {x}, {z}. Error: {value:?}");
                }

                let value = value.unwrap().unwrap();

                assert_eq!(value, (x as u128) << 8 | z as u128, "Invalid at {x}, {z}");
            }
        }
    }
}
