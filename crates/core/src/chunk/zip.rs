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

        zipped
    }
}
