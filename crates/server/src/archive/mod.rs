#![allow(unused)]

use std::{
    cell::{OnceCell, RefCell},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::FileExt,
};
use thiserror::Error;

const SECTOR_SIZE: usize = 4096;
const MAX_SECTORS_COUNT: usize = 32 * 32; // 32 chunks in each axis
const SECTOR_INDEX_SIZE: usize = 2 * 2; // 2 bytes, one for offset and another for sectors
const HEADER_SIZE: usize = SECTOR_INDEX_SIZE * MAX_SECTORS_COUNT;

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
}

#[derive(Default, Clone, Copy)]
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
        self.offset as u64 * SECTOR_SIZE as u64
    }

    fn bytes_count(&self) -> usize {
        Self::sector_to_bytes(self.sectors)
    }

    fn sectors_count(bytes: usize) -> u16 {
        let sector_size = SECTOR_SIZE as u16;
        (bytes as u16 + sector_size + 1) / sector_size
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
            sectors: vec![Default::default(); MAX_SECTORS_COUNT],
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
        ((x & 0xF) as usize) << 4 | (z & 0xF) as usize
    }
}

pub struct Archive<T> {
    header: Header,
    file_handler: File,
    _pd: std::marker::PhantomData<T>,
}

impl<T> Archive<T>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    pub fn new(name: &str) -> Result<Self, ArchiveError> {
        let mut file_handler = OpenOptions::new()
            .read(true)
            .write(true)
            .truncate(false)
            .create(true)
            .open(std::path::Path::new(name))?;

        let file_len = file_handler.seek(SeekFrom::End(0))?;

        let header = if file_len > 0 {
            let mut bytes = vec![0u8; HEADER_SIZE];
            file_handler.read_exact_at(&mut bytes, 0)?;
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
            archive.save_header()?;
        }

        Ok(archive)
    }

    pub fn read(&mut self, x: u8, z: u8) -> Result<Option<T>, ArchiveError> {
        let index = self.header.get_index(x, z);

        if index.is_empty() {
            return Ok(None);
        }

        self.file_handler
            .seek(SeekFrom::Start(index.seek_offset()))?;

        let mut frame = lz4_flex::frame::FrameDecoder::new(&self.file_handler);
        let value: T =
            bincode::serde::decode_from_std_read(&mut frame, bincode::config::standard())?;

        Ok(Some(value))
    }

    pub fn write(&mut self, x: u8, z: u8, value: &T) -> Result<(), ArchiveError> {
        let mut compressed = Vec::with_capacity(256 * 1024); // 256k
        let mut frame = lz4_flex::frame::FrameEncoder::new(&mut compressed);
        bincode::serde::encode_into_std_write(value, &mut frame, bincode::config::standard())?;
        frame.finish()?;

        let index = self.header.get_index(x, z);
        let needed_sectors = SectorIndex::sectors_count(compressed.len());

        // Check sector count
        let index = if index.is_empty() || needed_sectors > index.sectors {
            self.append(SectorIndex::sectors_count(compressed.len()))?
        } else {
            index
        };

        if index.is_empty() {
            return Err(ArchiveError::Write(format!(
                "Unable to find a index at {x}, {z}"
            )));
        }

        compressed.resize(SectorIndex::sector_to_bytes(needed_sectors), 0);

        self.header.set_index(x, z, index);
        self.file_handler
            .write_all_at(&compressed, index.seek_offset())?;

        Ok(())
    }

    pub fn save_header(&mut self) -> Result<(), ArchiveError> {
        let bytes = self.header.ser()?;
        self.file_handler.write_all_at(&bytes, 0)?;
        self.header.dirty = false;

        Ok(())
    }

    pub fn is_header_dirty(&self) -> bool {
        self.header.dirty
    }

    fn append(&mut self, needed_sectors: u16) -> Result<SectorIndex, ArchiveError> {
        let seek_position = self.file_handler.seek(SeekFrom::End(0))?;

        Ok(SectorIndex::from_seek_position(
            seek_position,
            needed_sectors,
        ))
    }
}

#[cfg(test)]
mod tests {
    use projekto_core::chunk::ChunkStorage;

    use super::*;

    fn temp_file() -> String {
        format!(
            "{}/projekto_{:#08}.tmp",
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
        let res = Archive::<u8>::new(&temp_file).unwrap();

        // Assert
        assert!(std::fs::exists(&temp_file).unwrap());

        let metadata = std::fs::metadata(&temp_file).unwrap();

        assert_eq!(metadata.len(), HEADER_SIZE as u64);
    }

    #[test]
    fn archive_new_existing_file_empty() {
        // Arrange
        let temp_file = temp_file();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&temp_file)
            .unwrap();
        drop(file);

        // Act
        let _res = Archive::<u8>::new(&temp_file).unwrap();

        // Assert
    }

    #[test]
    fn archive_new_existing_header_invalid() {
        // Arrange
        let temp_file = temp_file();
        std::fs::write(&temp_file, [0x00, 0xFF, 0xF1, 0x12]).unwrap();

        // Act
        let res = Archive::<u8>::new(&temp_file);

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
        let archive = Archive::<u8>::new(&temp_file).unwrap();

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

        let mut archive = Archive::<u8>::new(&temp_file).unwrap();
        archive
            .header
            .sectors
            .iter_mut()
            .enumerate()
            .for_each(|(i, index)| {
                index.offset = (MAX_SECTORS_COUNT - i) as u16;
                index.sectors = i as u16;
            });

        // Act
        archive.save_header().unwrap();
        drop(archive);
        let archive = Archive::<u8>::new(&temp_file).unwrap();

        // Assert
        archive
            .header
            .sectors
            .iter()
            .enumerate()
            .for_each(|(i, index)| {
                assert_eq!(index.offset, (MAX_SECTORS_COUNT - i) as u16);
                assert_eq!(index.sectors, i as u16);
            });
        assert!(!archive.is_header_dirty());
    }

    #[test]
    fn archive_read_write_single() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = Archive::<String>::new(&temp_file).unwrap();
        let txt = "The Silly Goosery is real!🪿︎";

        // Act
        archive.write(2, 3, &txt.to_string()).unwrap();
        let read_txt = archive.read(2, 3).unwrap();

        // Assert
        let read_txt = read_txt.unwrap();
        assert_eq!(read_txt, txt);
    }

    #[test]
    fn archive_write_same_sector_count() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = Archive::<Vec<u16>>::new(&temp_file).unwrap();
        archive.write(2, 3, &((0..20u16).collect())).unwrap();

        // Act
        let new_value = (0..1000u16).collect();
        archive.write(2, 3, &new_value).unwrap();

        // Assert
        let read_value = archive.read(2, 3).unwrap();
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
        let mut archive = Archive::<Vec<u16>>::new(&temp_file).unwrap();
        archive.write(2, 3, &((0..20u16).collect())).unwrap();
        let old_index = archive.header.get_index(2, 3);

        // Act
        let new_value = (0..4000u16).collect();
        archive.write(2, 3, &new_value).unwrap();

        // Assert
        let read_value = archive.read(2, 3).unwrap();
        let read_value = read_value.unwrap();
        assert_eq!(new_value, read_value);

        let new_index = archive.header.get_index(2, 3);
        assert_ne!(old_index.offset, new_index.offset);
        assert_ne!(old_index.sectors, new_index.sectors);
    }

    fn archive_() {
        // Arrange

        // Act

        // Assert
    }
}
