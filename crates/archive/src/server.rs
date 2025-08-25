use std::fmt::Debug;

use async_channel::{Receiver, Sender};
use bevy::{
    platform::collections::HashMap,
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use projekto_core::coords::{Chunk, Region, RegionChunk};

use crate::{Archive, ArchiveError};

/// Holds an active running task.
/// This is used to bridge the sync and async functions.
pub enum ArchiveTask<T> {
    /// A task for a load request. If there is no asset, `None` is returned.
    Load(Chunk, Receiver<Result<Option<T>, ArchiveError>>),
    /// A task for a save request.
    Save(Chunk, Receiver<Result<(), ArchiveError>>),
    /// A task for a save header request.
    SaveHeader(Chunk, Receiver<Result<(), ArchiveError>>),
}

impl<T> ArchiveTask<T> {
    /// Try to get the task result, if it has finished already.
    pub fn try_get_result(&self) -> Option<Result<Option<T>, ArchiveError>> {
        match self {
            ArchiveTask::Load(_, receiver) => receiver.try_recv().ok(),
            ArchiveTask::Save(_, receiver) | ArchiveTask::SaveHeader(_, receiver) => {
                // Nothing to return, so as long there is no error, it's fine to return None
                receiver.try_recv().ok().map(|_| Ok(None))
            }
        }
    }

    /// Returns the chunk this task refers to.
    pub fn chunk(&self) -> Chunk {
        match self {
            ArchiveTask::Load(chunk, _)
            | ArchiveTask::Save(chunk, _)
            | ArchiveTask::SaveHeader(chunk, _) => *chunk,
        }
    }
}

/// Archive commands to interact with async archives functions.
enum ArchiveCommand<T> {
    /// Request to load an asset, at the given coords. If there is no asset, `None` is returned.
    Load {
        chunk: RegionChunk,
        sender: Sender<Result<Option<T>, ArchiveError>>,
    },
    /// Request to save an asset at the given coords.
    Save {
        chunk: RegionChunk,
        asset: T,
        sender: Sender<Result<(), ArchiveError>>,
    },
    /// Request to save an archive header into disk.
    SaveHeader {
        sender: Sender<Result<(), ArchiveError>>,
    },
    /// Request to stop an archive worker.
    Stop,
}

impl<T> ArchiveCommand<T> {
    /// Create an `ArchiveCommand::Load` and returns a `Receiver` to wait for result.
    fn load(chunk: RegionChunk) -> (Receiver<Result<Option<T>, ArchiveError>>, Self) {
        let (sender, receiver) = async_channel::unbounded();

        (receiver, ArchiveCommand::Load { chunk, sender })
    }

    /// Create an `ArchiveCommand::Save` and returns a `Receiver` to wait for result.
    fn save(chunk: RegionChunk, asset: T) -> (Receiver<Result<(), ArchiveError>>, Self) {
        let (sender, receiver) = async_channel::unbounded();

        (
            receiver,
            ArchiveCommand::Save {
                chunk,
                asset,
                sender,
            },
        )
    }

    /// Create an `ArchiveCommand::SaveHeader` and returns a `Receiver` to wait for result.
    fn save_header() -> (Receiver<Result<(), ArchiveError>>, Self) {
        let (sender, receiver) = async_channel::unbounded();

        (receiver, ArchiveCommand::SaveHeader { sender })
    }
}

/// Returns the file path for the given region with the prefix.
fn get_region_path(prefix: &str, region: Region) -> String {
    format!("{prefix}{}_{}.rgn", region.x, region.z)
}

/// Starts an async worker which will listen the given receiver and process commands for the given
/// region.
///
/// This worker keeps running until `ArchiveCommand::Stop` is received or the receiver itself is
/// closed.
async fn start_worker<T>(root_folder: &str, region: Region, receiver: Receiver<ArchiveCommand<T>>)
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Debug,
{
    let archive = Archive::new(&get_region_path(root_folder, region)).await;

    let mut archive = if let Err(error) = archive {
        error!("Failed to load archive for region ({region:?}). Error: {error}");
        return;
    } else {
        archive.unwrap()
    };

    loop {
        if let Ok(cmd) = receiver.recv().await {
            match cmd {
                ArchiveCommand::Load { chunk, sender } => {
                    let result = archive.read(chunk).await;
                    let _ = sender.send(result).await;
                }

                ArchiveCommand::Save {
                    chunk,
                    asset,
                    sender,
                } => {
                    let _ = sender.send(archive.write(chunk, asset).await).await;
                }
                ArchiveCommand::SaveHeader { sender } => {
                    let _ = sender.send(archive.save_header().await).await;
                }
                ArchiveCommand::Stop => break,
            }
        } else {
            warn!("Archive worker ({region:?}) stopped. Channel closed.");
            break;
        }
    }
}

/// Represents an active archive worker, which receives commands and executes async tasks.
struct ArchiveWorker<T> {
    sender: Sender<ArchiveCommand<T>>,
    task: Task<()>,
}

pub type MaintenanceResult = Vec<(Region, Option<ArchiveError>)>;

/// The archive resource manager. This will server all requests and handle all the async
/// processing in order to fulfill the requests.
#[derive(Resource)]
pub struct ArchiveServer<T> {
    path: String,
    workers: HashMap<Region, ArchiveWorker<T>>,
}

impl<T> ArchiveServer<T> {
    /// Creates a new server for the given folder path.
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            workers: Default::default(),
        }
    }
}

impl<T> ArchiveServer<T>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + 'static + Debug,
{
    // /// Gets the local coords (inside the archive) for a given `Chunk`
    // fn to_local(chunk: Chunk) -> (u8, u8) {
    //     let x = chunk.x.rem_euclid(AXIS_CHUNK_SIZE as i32);
    //     let z = chunk.z.rem_euclid(AXIS_CHUNK_SIZE as i32);
    //
    //     assert!(x >= 0 && x <= AXIS_CHUNK_SIZE as i32);
    //     assert!(z >= 0 && z <= AXIS_CHUNK_SIZE as i32);
    //
    //     (x as u8, z as u8)
    // }

    /// Creates a new worker for the given region.
    fn new_region_worker(root_folder: &str, region: Region) -> ArchiveWorker<T> {
        let root_folder = root_folder.to_string();
        let (sender, receiver) = async_channel::unbounded();

        let task = IoTaskPool::get().spawn(async move {
            start_worker(&root_folder, region, receiver).await;
        });

        ArchiveWorker { sender, task }
    }

    /// Attempts to load a `Chunk`. Returns a task which yields the `Chunk` or `None`, if there is
    /// no chunk on the archive.
    pub fn load_chunk(&mut self, chunk: Chunk) -> Result<ArchiveTask<T>, ArchiveError> {
        let region = Region::from_chunk(chunk);
        let region_chunk = RegionChunk::from_chunk(chunk);

        let worker = self
            .workers
            .entry(region)
            .or_insert_with(|| Self::new_region_worker(&self.path, region));

        let (receiver, cmd) = ArchiveCommand::load(region_chunk);
        if let Err(e) = worker.sender.send_blocking(cmd) {
            return Err(ArchiveError::ChunkLoad(format!(
                "Failed to load chunk at {chunk}. Error: {e}"
            )));
        }

        Ok(ArchiveTask::Load(chunk, receiver))
    }

    /// Saves the `Chunk` into the archive.
    pub fn save_chunk(&mut self, chunk: Chunk, asset: T) -> Result<ArchiveTask<T>, ArchiveError> {
        let region = Region::from_chunk(chunk);
        let region_chunk = RegionChunk::from_chunk(chunk);

        let worker = self
            .workers
            .entry(region)
            .or_insert_with(|| Self::new_region_worker(&self.path, region));

        let (receiver, cmd) = ArchiveCommand::save(region_chunk, asset);
        if let Err(e) = worker.sender.send_blocking(cmd) {
            return Err(ArchiveError::ChunkSave(format!(
                "Failed to load save at {chunk}. Error: {e}"
            )));
        }

        Ok(ArchiveTask::Save(chunk, receiver))
    }

    /// Do needed maintenance stuff, like saving dirty headers into disk. This function should not
    /// be called every frame.
    ///
    /// Once every 1s~10s should be enough.
    pub fn do_maintenance_stuff(&mut self) -> Task<MaintenanceResult> {
        let workers = self
            .workers
            .iter()
            .map(|(region, worker)| (*region, worker.sender.clone()))
            .collect::<Vec<_>>();

        IoTaskPool::get().spawn(async move {
            let mut result = vec![];
            let mut receivers = vec![];

            for (region, sender) in workers {
                let (receiver, cmd) = ArchiveCommand::save_header();
                let _ = sender.send(cmd).await;
                receivers.push((region, receiver));
            }

            for (region, receiver) in receivers {
                if let Ok(res) = receiver.recv().await {
                    result.push((region, res.err()));
                }
            }

            result
        })
    }

    /// Remove the worker for the given region.
    pub fn remove_worker(&mut self, region: Region) {
        if let Some(worker) = self.workers.remove(&region) {
            IoTaskPool::get()
                .spawn(async move {
                    let _ = worker.sender.send(ArchiveCommand::Stop).await;
                    // Wait for task to finish
                    worker.task.await;
                })
                .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::tasks::{TaskPoolBuilder, block_on};

    use super::*;

    fn get_temp_path() -> String {
        format!("{}/projekto/test_", std::env::temp_dir().display())
    }

    fn create_test_archive(region: Region) {
        let path = get_region_path(&get_temp_path(), region);

        if std::fs::exists(&path).unwrap() {
            return;
        }

        let mut task = std::pin::pin!(Archive::<u128>::new(&path));
        let mut archive = block_on(&mut task).unwrap();

        for x in 0..Region::AXIS_SIZE as u8 {
            for z in 0..Region::AXIS_SIZE as u8 {
                let value = ((x as u128) << 8) | z as u128;
                let _ = block_on(archive.write(RegionChunk::new(x, z), value));
            }
        }

        let _ = block_on(archive.save_header());
    }

    #[test]
    fn to_region() {
        // Arrange
        let chunk = Chunk::new(-33, 44);

        // Act
        let region = Region::from_chunk(chunk);

        // Assert
        assert_eq!(region, Region::new(-2, 1));
    }

    #[test]
    fn to_local() {
        // Arrange
        let chunk = Chunk::new(-33, 44);

        // Act
        let region = RegionChunk::from_chunk(chunk);

        // Assert
        assert_eq!(region, RegionChunk::new(31, 12));
    }

    #[test]
    fn load_chunk() {
        // Arrange
        let _pool = IoTaskPool::get_or_init(|| TaskPoolBuilder::default().build());
        let chunk = Chunk::new(3, 4);
        let region = Region::from_chunk(chunk);
        create_test_archive(region);
        let mut server = ArchiveServer::<u128>::new(&get_temp_path());

        // Act
        let task = server.load_chunk(chunk).unwrap();

        // Assert
        let value = loop {
            if let Some(result) = task.try_get_result() {
                break result.unwrap().unwrap();
            }
        };

        assert_eq!(value, (((chunk.x as u128) << 8) | chunk.z as u128));
    }

    #[test]
    fn save_chunk() {
        // Arrange
        let _pool = IoTaskPool::get_or_init(|| TaskPoolBuilder::default().build());
        let chunk = Chunk::new(3, 4);

        let temp = format!(
            "{}/projekto/{:#08X}",
            std::env::temp_dir().display(),
            std::time::Instant::now().elapsed().as_micros()
        );

        let mut server = ArchiveServer::<u128>::new(&temp);

        // Act
        let task = server.save_chunk(chunk, 987654321).unwrap();
        while task.try_get_result().is_none() {
            std::thread::sleep(std::time::Duration::from_micros(100));
        }

        // Assert
        let task = server.load_chunk(chunk).unwrap();
        let value = loop {
            if let Some(result) = task.try_get_result() {
                break result.unwrap().unwrap();
            }
        };

        assert_eq!(value, 987654321);
    }

    #[test]
    fn do_maintenance_stuff_no_workers() {
        // Arrange
        let _pool = IoTaskPool::get_or_init(|| TaskPoolBuilder::default().build());

        let temp = format!(
            "{}/projekto/{:#08X}",
            std::env::temp_dir().display(),
            std::time::Instant::now().elapsed().as_micros()
        );

        let mut server = ArchiveServer::<u128>::new(&temp);

        // Act
        let task = server.do_maintenance_stuff();
        let result = block_on(task);

        // Assert
        assert!(result.is_empty());
    }

    #[test]
    fn do_maintenance_stuff() {
        // Arrange
        let _pool = IoTaskPool::get_or_init(|| TaskPoolBuilder::default().build());
        let mut tasks = vec![];

        let mut server = ArchiveServer::<u128>::new(&get_temp_path());
        let archives = 5;
        for x in 0..archives {
            let region = Region::new(x, 0);
            create_test_archive(region);

            tasks.push(
                server
                    .load_chunk(Chunk::new(region.x * Region::AXIS_SIZE as i32, 0))
                    .unwrap(),
            );
        }

        for task in tasks {
            if let ArchiveTask::Load(_, receiver) = task {
                receiver.recv_blocking().unwrap().unwrap();
            } else {
                panic!("Unvalid command type!");
            }
        }

        // Act
        let task = server.do_maintenance_stuff();
        let result = block_on(task);

        // Assert
        assert_eq!(result.len(), archives as usize);
    }
}
