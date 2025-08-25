//! This create is used to store asset inside an archive format file. This format is inspired
//! by the `fastanvil` crate, which is used to manipulate Minecraft anvil files.
//!
//! I made some adjustments to fit my needs, like making everything async and not having
//! timestamp info on header.
//!
//! The archive format is pretty straightforward, it has a header section, which has a index
//! for every possible asset inside the file. It is possible to store `Region::BUFFER_SIZE` assets
//! inside it.
//!
//! Each index in the header is a `SectorIndex`, which contains an offset and a sectors count.
//! The offset and also the sectors count are in `SECTOR_SIZE` units, which means to get the
//! final byte offset or size (in bytes), just multiplicate it with `SECTOR_SIZE`. An index with
//! offset zero, means there is no valid asset in file.
//!
//! Assets are serialized and compressed and a number of sectors is allocated at the end of file
//! (after `HEADER_SIZE` offset in bytes), so it can be stored. This means an empty archive has
//! only `HEADER_SIZE` of bytes in size.
//!
//! When reading an asset, the (x, z) position converted into an index, which is used to get the
//! respective `SectorIndex` from header. Then the file pointer is set to desired offset and the
//! number of bytes is read. After that, the data is decompressed and deserialized.
//!
//! When writing an asset, if it fits in the currently allocated sectors, the data is just written
//! on the disk, pretty much like when reading. If it needs more space, the new number of sectors
//! are allocated at the end of the file. This means the old sectors is "orphaned" and won't be
//! used anymore. In the future, it can be worth to add a "defrag" function, which would just
//! create a new archive, without those gaps.
use async_fs::{File, OpenOptions};
use bevy::prelude::*;
use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use projekto_core::coords::{Region, RegionChunk};
use std::io::SeekFrom;
use thiserror::Error;

mod server;
pub use server::{ArchiveServer, ArchiveTask, MaintenanceResult};

/// Size, in bytes, of each sector within the archive.
const SECTOR_SIZE: usize = 4096;
/// Size, in bytes, of each index within the archive.
const SECTOR_INDEX_SIZE: usize = 2 * 2; // 2 bytes, one for offset and another for sectors
/// Size, in bytes, of header section of the archive.
const HEADER_SIZE: usize = SECTOR_INDEX_SIZE * Region::BUFFER_SIZE;

/// Errors which can happens while using the archive crate.
/// Check variants for description.
#[derive(Debug, Error)]
pub enum ArchiveError {
    /// A general IO error happened. Usually this is related to the OS.
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    /// Raised when there is some error while decoding the asset from uncompressed bytes.
    #[error("Failed to decode: {0}")]
    Decode(#[from] bincode::error::DecodeError),
    /// Raised when there is some error while encoding the asset into bytes.
    #[error("Failed to encode: {0}")]
    Encode(#[from] bincode::error::EncodeError),
    /// Indicates a failure while compressing the encoded bytes.
    #[error("Failed to compress: {0}")]
    Compress(#[from] lz4_flex::frame::Error),
    /// Invalid number of bytes in the archive file. Usually means the file is corrupted.
    #[error("Invalid header size: {0}")]
    HeaderInvalid(usize),
    /// Raised when unable to write chunk into disk. Usually means some internal logic error.
    #[error("Failed to write: {0}")]
    Write(String),
    /// Raised when unable to save the chunk.
    #[error("Failed to load chunk: {0}")]
    ChunkLoad(String),
    /// Raised when unable to load the chunk.
    #[error("Failed to save chunk: {0}")]
    ChunkSave(String),
    /// Raised then some task was cancelled before returning the result.
    #[error("Failed to receive task result")]
    TaskRecv,
}

/// Represents a fat pointer to an asset, inside the archive.
#[derive(Default, Debug, Clone, Copy)]
struct SectorIndex {
    /// Offset, in `SECTOR_SIZE` units, of the beginning of the asset in the archive.
    offset: u16,
    /// Number of sectors allocated for this asset. Each sector has `SECTOR_SIZE` bytes.
    sectors: u16,
}

impl SectorIndex {
    /// Checks if this index points to a valid asset.
    /// An valid offset will always be after header, so zero means no asset.
    #[inline]
    fn is_empty(&self) -> bool {
        self.offset == 0
    }

    /// Returns the number of bytes this index offsets to, from the beginning of the file.
    #[inline]
    fn seek_offset(&self) -> u64 {
        assert_ne!(self.offset, 0);

        self.offset as u64 * SECTOR_SIZE as u64
    }

    /// Returns the total size of bytes this index points to.
    fn bytes_count(&self) -> usize {
        Self::sector_to_bytes(self.sectors)
    }

    /// Calculates the number of sectors needed to fit the given number of bytes.
    #[inline]
    fn sectors_count(bytes: usize) -> u16 {
        ((bytes + SECTOR_SIZE + 1) / SECTOR_SIZE) as u16
    }

    /// Creates a `SectorIndex` for the given seek position. This function will
    /// convert the seek position to `SECTOR_SIZE` units.
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

    /// Returns the bytes representation of self.
    #[inline]
    fn as_bytes(&self) -> [u8; 4] {
        ((self.offset as u32) << 16 | self.sectors as u32).to_be_bytes()
    }

    /// Creates an index from the bytes representation.
    fn from_bytes(bytes: [u8; 4]) -> Self {
        let i = u32::from_be_bytes(bytes);
        Self {
            offset: (i >> 16) as u16,
            sectors: (i & 0xFFFF) as u16,
        }
    }

    /// Converts the sector to absolute bytes.
    #[inline]
    fn sector_to_bytes(sectors: u16) -> usize {
        sectors as usize * SECTOR_SIZE
    }
}

/// The header of archive. Holds all assets indices.
/// When an index is created or update, it does not automatically writes to the disk.
/// This has to be done calling `Archive::save_header`.
#[derive(Default)]
struct Header {
    /// A list of all indices within the header.
    /// Even tho this is a `Vec`, it has always the fixed size of `MAX_CHUNK_COUNT` elements.
    sectors: Vec<SectorIndex>,
    /// Indicates if there was some change in sector indices.
    dirty: bool,
}

impl Header {
    /// Creates a new header with `MAX_CHUNK_COUNT` indices.
    fn new() -> Self {
        Self {
            sectors: vec![Default::default(); Region::BUFFER_SIZE],
            dirty: false,
        }
    }

    /// Deserialize a header from a byte slice.
    /// The slice must have at least `HEADER_SIZE` length
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

    /// Serialize header into a byte representation.
    /// It will always returns `HEADER_SIZE` number of bytes, otherwise an
    /// `ArchiveError::HeaderInvalid` is returned.
    fn ser(&self) -> Result<Vec<u8>, ArchiveError> {
        // TODO: It is possible to avoid this allocation by writing directly into the file writer
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

    /// Returns the `SectorIndex` at the given chunk.
    fn get_index(&self, chunk: RegionChunk) -> SectorIndex {
        self.sectors[chunk.to_index()]
    }

    /// Sets the given index at the given chunk.
    /// This marks the headers as dirty.
    fn set_index(&mut self, chunk: RegionChunk, index: SectorIndex) {
        self.sectors[chunk.to_index()] = index;
        self.dirty = true;
    }
}

/// This struct represents an archive file.
/// Check crate level docs for more info.
pub struct Archive<T> {
    /// The header of file
    header: Header,
    /// An async file handler. Used to do OS operations.
    file_handler: File,
    _pd: std::marker::PhantomData<T>,
}

impl<T> Archive<T> {
    /// Creates a new archive for the given file path.
    ///
    /// This function creates the whole path sub folders, if it doesn't exists.
    ///
    /// If there is no file, a new one is created, saving the new header.
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

    /// Save the header into disk.
    ///
    /// This needs to be done manually due to performance reasons.
    pub async fn save_header(&mut self) -> Result<(), ArchiveError> {
        let bytes = self.header.ser()?;
        self.file_handler.seek(SeekFrom::Start(0)).await?;
        self.file_handler.write_all(&bytes).await?;
        self.file_handler.flush().await?;
        self.header.dirty = false;

        Ok(())
    }

    /// Checks if the header was modified since it was last saved or loaded.
    pub fn is_header_dirty(&self) -> bool {
        self.header.dirty
    }
}

impl<T> Archive<T>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    /// Read an asset at the given coords. If there is no asset, `None` is returned.
    pub async fn read(&mut self, chunk: RegionChunk) -> Result<Option<T>, ArchiveError> {
        let index = self.header.get_index(chunk);

        if index.is_empty() {
            return Ok(None);
        }

        let mut buffer = vec![0u8; index.bytes_count()];

        self.file_handler
            .seek(SeekFrom::Start(index.seek_offset()))
            .await?;

        self.file_handler.read_exact(&mut buffer).await?;

        // TODO: It is possible to avoid allocating a buffer, but both lz4_flex and bincode aren't
        // compatible with async.
        let mut frame = lz4_flex::frame::FrameDecoder::new(&*buffer);
        let value: T =
            bincode::serde::decode_from_std_read(&mut frame, bincode::config::standard())?;

        Ok(Some(value))
    }

    /// Write the given asset at the given coords.
    ///
    /// Attempts to write on the current allocated sectors. If it doesn't fit, a new number of
    /// sectors are allocated at the end of the archive.
    pub async fn write(&mut self, chunk: RegionChunk, value: T) -> Result<(), ArchiveError> {
        // TODO: It is possible to avoid allocating a buffer, but both lz4_flex and bincode aren't
        // compatible with async.
        let mut compressed = Vec::with_capacity(256 * 1024); // 256k
        let mut frame = lz4_flex::frame::FrameEncoder::new(&mut compressed);
        bincode::serde::encode_into_std_write(&value, &mut frame, bincode::config::standard())?;
        frame.finish()?;

        let index = self.header.get_index(chunk);
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
                "Unable to find a index at {chunk}"
            )));
        }

        let new_len = SectorIndex::sector_to_bytes(needed_sectors);
        compressed.resize(new_len, 0);

        self.header.set_index(chunk, index);
        self.file_handler
            .seek(SeekFrom::Start(index.seek_offset()))
            .await?;
        self.file_handler.write_all(&compressed).await?;
        self.file_handler.flush().await?;

        Ok(())
    }

    /// Creates a new `SectorIndex` pointing to the end of the archive.
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
    fn header_size_matches_buffer_size() {
        assert_eq!(
            HEADER_SIZE % SECTOR_SIZE,
            0,
            "HEADER_SIZE must be multiple of SECTOR_SIZE"
        );
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
                    RegionChunk::new(x as u8, z as u8),
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
                let chunk = RegionChunk::new(x as u8, z as u8);
                let index = header.get_index(chunk);

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
        let _res = block_on(Archive::<u8>::new(&temp_file)).unwrap();

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
                index.offset = (Region::BUFFER_SIZE - i) as u16;
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
                assert_eq!(index.offset, (Region::BUFFER_SIZE - i) as u16);
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
        block_on(archive.write(RegionChunk::new(2, 3), txt.to_string())).unwrap();
        let read_txt = block_on(archive.read(RegionChunk::new(2, 3))).unwrap();

        // Assert
        let read_txt = read_txt.unwrap();
        assert_eq!(read_txt, txt);
    }

    #[test]
    fn archive_write_same_sector_count() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = block_on(Archive::<Vec<u16>>::new(&temp_file)).unwrap();
        block_on(archive.write(RegionChunk::new(2, 3), (0..20u16).collect())).unwrap();

        // Act
        let new_value = (0..1000u16).collect::<Vec<_>>();
        block_on(archive.write(RegionChunk::new(2, 3), new_value.clone())).unwrap();

        // Assert
        let read_value = block_on(archive.read(RegionChunk::new(2, 3))).unwrap();
        let read_value = read_value.unwrap();
        assert_eq!(new_value, read_value);

        let index = archive.header.get_index(RegionChunk::new(2, 3));
        assert_eq!(index.offset, 1);
        assert_eq!(index.sectors, 1);
    }

    #[test]
    fn archive_write_append_sector() {
        // Arrange
        let temp_file = temp_file();
        let mut archive = block_on(Archive::<Vec<u16>>::new(&temp_file)).unwrap();
        block_on(archive.write(RegionChunk::new(2, 3), (0..20u16).collect())).unwrap();
        let old_index = archive.header.get_index(RegionChunk::new(2, 3));

        // Act
        let new_value = (0..4000u16).collect::<Vec<_>>();
        block_on(archive.write(RegionChunk::new(2, 3), new_value.clone())).unwrap();

        // Assert
        let read_value = block_on(archive.read(RegionChunk::new(2, 3))).unwrap();
        let read_value = read_value.unwrap();
        assert_eq!(new_value, read_value);

        let new_index = archive.header.get_index(RegionChunk::new(2, 3));
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
            chunks.push(x);
            block_on(archive.write(RegionChunk::new(x, 0), chunk)).unwrap();
        }

        // Assert
        for x in chunks {
            let chunk = generate_chunk(x as u64);
            let cached_chunk = block_on(archive.read(RegionChunk::new(x, 0)));

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
        for x in 30..Region::AXIS_SIZE as u8 {
            for z in 0..Region::AXIS_SIZE as u8 {
                let value = (x as u128) << 8 | z as u128;
                block_on(archive.write(RegionChunk::new(x, z), value)).unwrap();
            }
        }

        // Assert
        for x in 30..Region::AXIS_SIZE as u8 {
            for z in 0..Region::AXIS_SIZE as u8 {
                let value = block_on(archive.read(RegionChunk::new(x, z)));

                if value.is_err() {
                    panic!("Failed at {x}, {z}. Error: {value:?}");
                }

                let value = value.unwrap().unwrap();

                assert_eq!(value, (x as u128) << 8 | z as u128, "Invalid at {x}, {z}");
            }
        }
    }
}
