use std::{marker::PhantomData, ops::Deref};

use bevy::{
    ecs::system::SystemParam,
    prelude::*,
    reflect::TypeUuid,
    tasks::{AsyncComputeTaskPool, Task},
    utils::HashMap,
};
use futures_lite::future;

use projekto_core::{
    chunk::Chunk,
    voxel::{self},
    VoxWorld,
};

mod resources;
mod task;

pub use resources::*;

const CACHE_PATH: &str = "cache/chunks/";
const CACHE_EXT: &str = "bin";

pub(super) struct GenesisPlugin;

impl Plugin for GenesisPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EvtChunkUpdated>()
            .init_resource::<RunningTask>()
            .init_resource::<ChunkKindRes>()
            .init_resource::<ChunkLightRes>()
            .init_resource::<ChunkVertexRes>()
            .add_system_set_to_stage(
                CoreStage::PreUpdate,
                SystemSet::new()
                    .with_system(collect_completed_task_results)
                    .label(GenesisLabel::Collect),
            )
            .add_system_set_to_stage(
                CoreStage::PostUpdate,
                SystemSet::new()
                    .with_system(dispatch_task)
                    .label(GenesisLabel::Dispatch),
            )
            .add_startup_system_to_stage(StartupStage::PreStartup, setup_resources);
    }
}

/// [`SystemSet`] labels used by [`GenesisPlugin`] to do interact with [`VoxWorld`]
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel, Reflect)]
pub enum GenesisLabel {
    /// Collects all tasks results and place them in resource
    /// This happens at [`CoreStage::PreUpdate`]
    Collect,
    /// Dispatch all update requests to async task.
    /// This happens at [`CoreStage::PostUpdate`]
    Dispatch,
}

#[derive(TypeUuid, Debug)]
#[uuid = "e6edff2a-e204-497f-999c-bdebd1f92f62"]
pub struct KindsAtlasRes {
    pub atlas: Handle<Image>,
}

pub struct EvtChunkUpdated(pub IVec3);

fn setup_resources(mut commands: Commands, asset_server: Res<AssetServer>) {
    trace_system_run!();

    if !std::path::Path::new(CACHE_PATH).exists() {
        std::fs::create_dir_all(CACHE_PATH).unwrap();
    }

    let vox_world = VoxWorld::default();
    commands.insert_resource(WorldRes(Some(vox_world)));

    let kinds_path = format!("{}{}", env!("ASSETS_PATH"), "/voxels/kind.ron");
    let descs = voxel::KindsDescs::init(kinds_path);

    let atlas = asset_server.load(&descs.atlas_path);

    commands.insert_resource(KindsAtlasRes { atlas });

    commands.insert_resource(BatchChunkCmdRes::default());
}

/**
Hold chunk commands to be processed in batch.
Internally uses a double buffered list of commands to keep track of what is running and what is pending.
*/
#[derive(Default)]
pub struct BatchChunkCmdRes {
    pending: Vec<ChunkCmd>,
    running: Vec<ChunkCmd>,
}

impl BatchChunkCmdRes {
    /**
    Swap the running and pending buffers

    Returns a clone of the running buffer
     */
    fn swap_and_clone(&mut self) -> Vec<ChunkCmd> {
        // Since the running buffer is always cleared when the batch is finished, this swap has no side-effects
        std::mem::swap(&mut self.running, &mut self.pending);
        self.running.clone()
    }

    /**
    Clears the running buffer
    */
    fn finished(&mut self) -> Vec<ChunkCmd> {
        std::mem::take(&mut self.running)
    }

    /**
    Adds a load command to the batch
     */
    pub fn load(&mut self, local: IVec3) {
        self.pending.push(ChunkCmd::Load(local));
    }

    /**
    Adds an unload command to the batch
     */
    pub fn unload(&mut self, local: IVec3) {
        self.pending.push(ChunkCmd::Unload(local));
    }

    /**
    Adds an update command to the batch
     */
    pub fn update(&mut self, local: IVec3, voxels: Vec<(IVec3, voxel::Kind)>) {
        self.pending.push(ChunkCmd::Update(local, voxels));
    }

    fn count_chunk_cmd(vec: &Vec<ChunkCmd>) -> (i32, i32, i32) {
        vec.iter()
            .map(|c| match &c {
                ChunkCmd::Load(_) => (1, 0, 0),
                ChunkCmd::Unload(_) => (0, 1, 0),
                ChunkCmd::Update(_, _) => (0, 0, 1),
            })
            .fold((0, 0, 0), |s, v| (s.0 + v.0, s.1 + v.1, s.2 + v.2))
    }
}

impl std::fmt::Display for BatchChunkCmdRes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (pending_load, pending_unload, pending_update) = Self::count_chunk_cmd(&self.pending);
        let (running_load, running_unload, running_update) = Self::count_chunk_cmd(&self.running);

        write!(
            f,
            "Running LD: {} UL: {} UP: {} | Pending LD: {} UL: {} UP: {}",
            running_load,
            running_unload,
            running_update,
            pending_load,
            pending_unload,
            pending_update,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ChunkCmd {
    Load(IVec3),
    Unload(IVec3),
    Update(IVec3, Vec<(IVec3, voxel::Kind)>),
}

struct WorldRes(Option<VoxWorld>);

impl WorldRes {
    pub fn take(&mut self) -> VoxWorld {
        self.0
            .take()
            .expect("You can take world only when it's ready")
    }

    pub fn set(&mut self, world: VoxWorld) {
        assert!(
            self.0.replace(world).is_none(),
            "There can be only one world at a time"
        );
    }
}

impl Deref for WorldRes {
    type Target = VoxWorld;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("WorldRes should be ready")
    }
}

#[derive(Default, Deref, DerefMut)]
struct RunningTask(pub Option<Task<(VoxWorld, Vec<IVec3>)>>);

#[derive(SystemParam)]
struct ChunkResources<'w, 's> {
    kind: ResMut<'w, ChunkKindRes>,
    light: ResMut<'w, ChunkLightRes>,
    vertex: ResMut<'w, ChunkVertexRes>,

    #[system_param(ignore)]
    _pd: PhantomData<&'s ()>,
}

impl<'w, 's> ChunkResources<'w, 's> {
    fn set(&mut self, local: IVec3, chunk: &Chunk) {
        let Chunk {
            kinds,
            lights,
            vertices,
        } = chunk.clone();

        self.kind.insert(local, kinds);
        self.light.insert(local, lights);
        self.vertex.insert(local, vertices);
    }

    fn remove(&mut self, local: IVec3) {
        self.kind.remove(&local);
        self.light.remove(&local);
        self.vertex.remove(&local);
    }
}

fn collect_completed_task_results(
    mut running_task: ResMut<RunningTask>,
    mut world_res: ResMut<WorldRes>,
    mut batch_res: ResMut<BatchChunkCmdRes>,
    mut updated_writer: EventWriter<EvtChunkUpdated>,
    mut chunk_resources: ChunkResources,
) {
    if let Some(ref mut task) = **running_task {
        // Check if task has finished
        if let Some((world, updated_list)) = future::block_on(future::poll_once(task)) {
            debug!("Completed task. Updated chunks: {}", updated_list.len());

            // Dispatch all events generated from this batch
            updated_list
                .into_iter()
                .for_each(|local| updated_writer.send(EvtChunkUpdated(local)));

            // Update chunk resources with the outcome of task
            let finished = batch_res.finished();
            for cmd in finished {
                match cmd {
                    ChunkCmd::Load(local) | ChunkCmd::Update(local, _) => {
                        chunk_resources.set(local, world.get(local).unwrap())
                    }
                    ChunkCmd::Unload(i) => chunk_resources.remove(i),
                }
            }

            // Give back the VoxWorld to WorldRes
            **running_task = None;
            world_res.set(world);
        }
    }
}

fn dispatch_task(
    mut running_task: ResMut<RunningTask>,
    mut batch_res: ResMut<BatchChunkCmdRes>,
    mut world_res: ResMut<WorldRes>,
) {
    if running_task.is_some() {
        // Wait for current task to finish
        return;
    }
    let commands = batch_res.swap_and_clone();
    let commands = optimize_commands(&world_res, commands);

    if !commands.is_empty() {
        let world = world_res.take();

        **running_task =
            Some(AsyncComputeTaskPool::get().spawn(task::process_batch(world, commands)));
    } else {
        batch_res.finished();
    }
}

/**
 * System which process pending commands and updates the world.
 *
 * Since there can be only one copy of World at any given time, when this system process commands
 * it takes ownership of the [`VoxWorld`] from [`WorldRes`] until all the batch is processed.
 *
 * This can take several frames.
 */
// fn update_world_system(
//     mut batch_res: ResMut<BatchChunkCmdRes>,
//     mut meta: Local<RunningTask>,
//     mut world_res: ResMut<WorldRes>,
//     mut updated_writer: EventWriter<EvtChunkUpdated>,
// ) {
//     let mut _perf = perf_fn!();

//     if let Some(ref mut task) = meta.running_task {
//         perf_scope!(_perf);

//         // Check if task has finished
//         if let Some((world, updated_list)) = future::block_on(future::poll_once(task)) {
//             // Dispatch all events generated from this batch
//             updated_list
//                 .into_iter()
//                 .for_each(|local| updated_writer.send(EvtChunkUpdated(local)));

//             // Give back the VoxWorld to WorldRes
//             meta.running_task = None;
//             world_res.set(world);
//             batch_res.finished();
//         }
//     } else if batch_res.has_pending_commands() {
//         perf_scope!(_perf);

//         let world = world_res.take();
//         let batch = batch_res.swap_and_clone();

//         let task_pool = AsyncComputeTaskPool::get();
//         meta.running_task = Some(task_pool.spawn(process_batch(world, batch)));
//     }

//     assert_ne!(
//         meta.running_task.is_none(),
//         !world_res.is_ready(),
//         "The world should exists only in one place at any given time"
//     );
//     assert_ne!(
//         meta.running_task.is_some(),
//         world_res.is_ready(),
//         "The world should exists only in one place at any given time"
//     );
// }

/**
This functions optimize the command list removing duplicated commands or commands that nullifies each other.

**Rules**
 1. Skips any duplicated commands (*Load* -> *Load*, *Update* -> *Update*, *Unload* -> *Unload*).
 2. Skips *Load* and remove existing *Unload* cmd when chunk exists already.
 3. Skips *Unload* and remove existing *Load* cmd when chunk doesn't exists already.
 4. Skips *Unload* when chunk doesn't exists already.
 5. Skips *Load* when chunk exists already.
 6. Skips *Update* if the chunk doesn't exists already.
 7. Replaces *Update* by *Unload* if the chunk exists already.
 8. Replaces *Update* by *Load* if the chunk doesn't exists already. [Removed]
 9. Skips *Update* if there is an *Unload* cmd already.

**This functions does preserves the insertion order**

**Returns** an optimized command list
*/
fn optimize_commands(world: &VoxWorld, commands: Vec<ChunkCmd>) -> Vec<ChunkCmd> {
    perf_fn_scope!();

    let mut map = HashMap::<IVec3, (u32, ChunkCmd)>::new();

    // Used to preserve command insertion order
    let mut order = 0u32;

    for cmd in commands {
        match cmd {
            ChunkCmd::Load(local) => {
                let chunk_exists = world.get(local).is_some();

                if let Some((_, existing_cmd)) = map.get(&local) {
                    match existing_cmd {
                        ChunkCmd::Load(_) => continue, // Rule 1
                        ChunkCmd::Unload(_) if chunk_exists => {
                            // Rule 2
                            map.remove(&local);
                            continue;
                        }
                        _ => {
                            panic!(
                                "Undefined behavior for {:?} and {:?} when chunk_exists = {:?}",
                                cmd, existing_cmd, chunk_exists
                            );
                        }
                    }
                } else if chunk_exists {
                    // Rule 5
                    continue;
                }

                order += 1;
                let existing = map.insert(local, (order, cmd));

                debug_assert!(existing.is_none(), "This should never happens, since all existing cases should be handled by above match");
            }
            ChunkCmd::Unload(local) => {
                let chunk_exists = world.get(local).is_some();

                if let Some((_, existing_cmd)) = map.get(&local) {
                    match existing_cmd {
                        ChunkCmd::Unload(_) => continue, // Rule 1
                        ChunkCmd::Load(_) if !chunk_exists => {
                            // Rule 3
                            map.remove(&local);
                            continue;
                        }
                        ChunkCmd::Update(_, _) if chunk_exists => {
                            // Rule 7
                            order += 1;
                            map.insert(local, (order, cmd.clone()));
                            continue;
                        }
                        _ => {
                            panic!(
                                "Undefined behavior for {:?} and {:?} when chunk_exists = {:?}",
                                cmd, existing_cmd, chunk_exists
                            );
                        }
                    }
                } else if !chunk_exists {
                    // Rule 4
                    continue;
                }

                order += 1;
                let existing = map.insert(local, (order, cmd));

                debug_assert!(existing.is_none(), "This should never happens, since all existing cases should be handled by above match");
            }
            ChunkCmd::Update(local, _) => {
                if world.get(local).is_none() {
                    // Rule 6
                    continue;
                }

                if let Some((_, existing_cmd)) = map.get(&local) {
                    match existing_cmd {
                        ChunkCmd::Update(_, _) => continue, // Rule 1. TODO: Maybe merge update data in the future?
                        ChunkCmd::Unload(_) => continue,    // Rule 9.
                        _ => {
                            panic!("Undefined behavior for {:?} and {:?}", cmd, existing_cmd);
                        }
                    }
                }

                order += 1;
                let existing = map.insert(local, (order, cmd));

                debug_assert!(existing.is_none(), "This should never happens, since all existing cases should be handled by above match");
            }
        }
    }

    let mut values = map.values().collect::<Vec<_>>();
    values.sort_by(|&t1, &t2| t1.0.cmp(&t2.0));

    values.into_iter().map(|(_, cmd)| cmd.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimize_commands_preserve_insertion_order() {
        let cmds = (0..100)
            .into_iter()
            .map(|i| ChunkCmd::Load((i, i, i).into()))
            .collect::<Vec<_>>();

        let optimized = super::optimize_commands(&VoxWorld::default(), cmds.clone());

        assert_eq!(cmds, optimized);
    }

    #[test]
    fn optimize_commands_rule_1() {
        let cmds = vec![
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Load((1, 2, 1).into()),
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Load((1, 2, 1).into()),
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Load((1, 3, 1).into()),
            ChunkCmd::Load((1, 2, 1).into()),
        ];
        let world = VoxWorld::default();

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(
            optimized,
            vec![
                ChunkCmd::Load((1, 1, 1).into()),
                ChunkCmd::Load((1, 2, 1).into()),
                ChunkCmd::Load((1, 3, 1).into()),
            ]
        );
    }

    #[test]
    fn optimize_commands_rule_2() {
        let cmds = vec![
            ChunkCmd::Unload((1, 1, 1).into()),
            ChunkCmd::Load((1, 1, 1).into()),
        ];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_3() {
        let cmds = vec![
            ChunkCmd::Load((1, 1, 1).into()),
            ChunkCmd::Unload((1, 1, 1).into()),
        ];
        let world = VoxWorld::default();

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_4() {
        let cmds = vec![ChunkCmd::Unload((1, 1, 1).into())];
        let world = VoxWorld::default();

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_5() {
        let cmds = vec![ChunkCmd::Load((1, 1, 1).into())];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_6() {
        let cmds = vec![ChunkCmd::Update((1, 1, 1).into(), vec![])];
        let world = VoxWorld::default();

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![]);
    }

    #[test]
    fn optimize_commands_rule_7() {
        let cmds = vec![
            ChunkCmd::Update((1, 1, 1).into(), vec![]),
            ChunkCmd::Unload((1, 1, 1).into()),
        ];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![ChunkCmd::Unload((1, 1, 1).into())]);
    }

    #[test]
    fn optimize_commands_rule_9() {
        let cmds = vec![
            ChunkCmd::Unload((1, 1, 1).into()),
            ChunkCmd::Update((1, 1, 1).into(), vec![]),
        ];
        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(optimized, vec![ChunkCmd::Unload((1, 1, 1).into())]);
    }

    #[test]
    fn optimize_commands_all_rules() {
        let cmds = vec![
            ChunkCmd::Load((0, 0, 0).into()),
            ChunkCmd::Load((1, 1, 1).into()),   // Skipped by Rule 1
            ChunkCmd::Unload((1, 1, 1).into()), // Removed by Rule 2
            ChunkCmd::Load((1, 1, 1).into()),   // Skipped by Rule 2
            ChunkCmd::Update((1, 1, 1).into(), vec![]),
            ChunkCmd::Load((1, 2, 1).into()),   // Removed by Rule 3
            ChunkCmd::Unload((1, 2, 1).into()), // Skipped by Rule 3
            ChunkCmd::Unload((1, 2, 1).into()), // Skipped by rule 4
            ChunkCmd::Load((1, 3, 1).into()),   // Skipped by Rule 5
            ChunkCmd::Update((1, 4, 1).into(), vec![]), // Skipped by Rule 6
            ChunkCmd::Update((1, 5, 1).into(), vec![]), // Replaced by Rule 7
            ChunkCmd::Update((1, 5, 1).into(), vec![]), // Replaced by Rule 1
            ChunkCmd::Update((1, 5, 1).into(), vec![]), // Replaced by Rule 1
            ChunkCmd::Unload((1, 5, 1).into()),
            ChunkCmd::Unload((1, 6, 1).into()),
            ChunkCmd::Update((1, 6, 1).into(), vec![]), // Skipped by Rule 9
        ];

        let mut world = VoxWorld::default();
        world.add((1, 1, 1).into(), Default::default());
        world.add((1, 3, 1).into(), Default::default());
        world.add((1, 5, 1).into(), Default::default());
        world.add((1, 6, 1).into(), Default::default());

        let optimized = super::optimize_commands(&world, cmds.clone());

        assert_eq!(
            optimized,
            vec![
                ChunkCmd::Load((0, 0, 0).into()),
                ChunkCmd::Update((1, 1, 1).into(), vec![]),
                ChunkCmd::Unload((1, 5, 1).into()),
                ChunkCmd::Unload((1, 6, 1).into())
            ]
        );
    }
}
