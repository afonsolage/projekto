use serde::{Deserialize, Serialize};

use crate::chunk::{self, ChunkStorage, ChunkStorageType};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
pub struct ChunkRun<T> {
    value: T,
    /// Shameless Hack:
    /// Since this can never be 0 and in order to fit a ``u16::MAX``
    /// the count starts from 0, not 1.
    count: u16,
}

pub fn decompress<T: ChunkStorageType>(rle: Vec<ChunkRun<T>>) -> ChunkStorage<T> {
    let mut chunk = ChunkStorage::default();

    let mut index = 0usize;
    for run in rle {
        for _ in 0..=run.count as usize {
            chunk[index] = run.value;
            index += 1;
        }
    }

    chunk
}

pub fn compress<T: ChunkStorageType>(storage: ChunkStorage<T>) -> Vec<ChunkRun<T>> {
    let mut rle = vec![];

    let mut current = ChunkRun {
        value: storage[0],
        count: 0,
    };

    for index in 1..chunk::BUFFER_SIZE {
        if storage[index] == current.value && current.count < u16::MAX {
            current.count += 1;
        } else {
            rle.push(current);

            current = ChunkRun {
                value: storage[index],
                count: 0,
            };
        }
    }
    rle.push(current);

    rle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_empty_chunk() {
        // Arrange
        let empty = ChunkStorage::<u8>::default();

        // Act
        let rle = compress(empty);

        // Assert
        assert_eq!(rle.len(), 1, "There should be only one run on rle");
        assert_eq!(rle[0].value, 0, "The value should be the default u8 one");
        assert_eq!(
            (rle[0].count as usize) + 1, // count starts from 0, so add 1 to match
            chunk::BUFFER_SIZE,
            "The entire chunk should be compressed",
        );
    }

    #[test]
    fn compresss_chunk() {
        // Arrange
        let mut chunk = ChunkStorage::<u8>::default();
        chunk[0] = 1;
        chunk[99] = 2;
        chunk[100] = 3;
        chunk[101] = 3;

        for i in 102..chunk::BUFFER_SIZE {
            chunk[i] = 1;
        }

        // Act
        let rle = compress(chunk);

        // Assert
        assert_eq!(rle.len(), 5, "There should be 5 runs ({rle:?})");

        assert_eq!(rle[0], ChunkRun { value: 1, count: 0 });

        assert_eq!(
            rle[1],
            ChunkRun {
                value: 0,
                count: 97
            }
        );

        assert_eq!(rle[2], ChunkRun { value: 2, count: 0 });

        assert_eq!(rle[3], ChunkRun { value: 3, count: 1 });

        assert_eq!(
            rle[4],
            ChunkRun {
                value: 1,
                count: (chunk::BUFFER_SIZE - 103) as u16
            }
        );
    }

    #[test]
    fn decompress_empty_chunk() {
        // Arrange
        let rle = vec![ChunkRun {
            value: 0,
            count: (chunk::BUFFER_SIZE - 1) as u16,
        }];

        // Act
        let storage = decompress(rle);

        // Assert
        for i in 0..chunk::BUFFER_SIZE {
            assert_eq!(storage[i], u8::default());
        }
    }

    #[test]
    fn decompress_chunk() {
        // Arrange
        let rle = vec![
            ChunkRun { value: 1, count: 2 },
            ChunkRun { value: 2, count: 0 },
            ChunkRun {
                value: 3,
                count: (chunk::BUFFER_SIZE - 5) as u16,
            },
        ];

        // Act
        let storage = decompress(rle);

        // Assert
        assert_eq!(storage[0], 1);
        assert_eq!(storage[1], 1);
        assert_eq!(storage[2], 1);

        assert_eq!(storage[3], 2);

        for i in 4..chunk::BUFFER_SIZE {
            assert_eq!(storage[i], 3);
        }
    }
}
