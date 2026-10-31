use crate::chunk::{self, ChunkStorage, ChunkStorageType};

impl<T> ChunkStorage<T>
where
    T: ChunkStorageType,
{
    pub fn zip<O>(&self, other: &ChunkStorage<O>) -> ChunkStorage<(T, O)>
    where
        O: ChunkStorageType,
        (T, O): ChunkStorageType,
    {
        let mut zipped = ChunkStorage::default();

        for v in chunk::voxels() {
            let a = self.get(v);
            let b = other.get(v);
            zipped.set(v, (a, b));
        }

        zipped.pack();

        zipped
    }

    pub fn zip_2<O1, O2>(
        &self,
        other_1: &ChunkStorage<O1>,
        other_2: &ChunkStorage<O2>,
    ) -> ChunkStorage<(T, O1, O2)>
    where
        O1: ChunkStorageType,
        O2: ChunkStorageType,
        (T, O1, O2): ChunkStorageType,
    {
        let mut zipped = ChunkStorage::default();

        for v in chunk::voxels() {
            let a = self.get(v);
            let b = other_1.get(v);
            let c = other_2.get(v);
            zipped.set(v, (a, b, c));
        }

        zipped.pack();

        zipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::*;

    #[test]
    fn test_zip() {
        let mut s1 = ChunkStorage::default();
        let mut s2 = ChunkStorage::default();

        for (i, v) in chunk::voxels().enumerate() {
            let value = (i % 255) as u16;
            s1.set(v, value as u8);
            s2.set(v, value * 2);
        }

        let zipped = s1.zip(&s2);

        for (i, v) in chunk::voxels().enumerate() {
            let value = (i % 255) as u16;
            let (a, b) = zipped.get(v);
            assert_eq!(a, value as u8);
            assert_eq!(b, value * 2);
        }
    }

    #[test]
    fn test_zip_2() {
        let mut s1 = ChunkStorage::default();
        let mut s2 = ChunkStorage::default();
        let mut s3 = ChunkStorage::default();

        for (i, v) in chunk::voxels().enumerate() {
            let value = (i % 255) as u16;
            s1.set(v, value as u8);
            s2.set(v, value * 2);
            s3.set(v, value * 3);
        }

        let zipped = s1.zip_2(&s2, &s3);

        for (i, v) in chunk::voxels().enumerate() {
            let value = (i % 255) as u16;
            let (a, b, c) = zipped.get(v);
            assert_eq!(a, value as u8);
            assert_eq!(b, value * 2);
            assert_eq!(c, value * 3);
        }
    }
}
