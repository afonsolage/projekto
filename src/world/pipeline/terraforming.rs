use std::collections::VecDeque;

use bevy::{ecs::system::SystemParam, prelude::*, utils::HashMap};

use crate::world::storage::{chunk::ChunkKind, voxel};

use super::{genesis::BatchChunkCmdRes, WorldRes};

pub(super) struct TerraformingPlugin;

impl Plugin for TerraformingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CmdChunkUpdate>()
            .add_event::<EvtChunkUpdatedOld>()
            .add_startup_system_to_stage(super::PipelineStartup::Terraforming, setup_resources)
            .add_system_set_to_stage(
                super::Pipeline::Terraforming,
                SystemSet::new()
                    .with_system(process_update_chunks_system)
                    .with_system(some_query_system)
                    .with_system(another_system),
            );
    }
}

fn setup_resources(mut commands: Commands) {
    commands.insert_resource(ChunkQuery::default());
}

#[derive(Clone)]
pub struct CmdChunkUpdate(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

#[derive(Clone, Copy)]
pub struct EvtChunkUpdatedOld(pub IVec3);

fn process_update_chunks_system(
    mut reader: EventReader<CmdChunkUpdate>,
    mut batch: ResMut<BatchChunkCmdRes>,
) {
    let mut _perf = perf_fn!();

    for CmdChunkUpdate(local, voxels) in reader.iter() {
        batch.update(*local, voxels.clone());
    }
}

fn another_system(world: Res<WorldRes>, mut query_res: ResMut<ChunkQuery>) {
    if !world.is_ready() {
        return;
    }

    let requests = query_res.queries.drain().collect::<Vec<_>>();

    for (id, chunks) in requests {
        info!("Setting result for query {} {:?}", id, chunks);

        let result = chunks
            .iter()
            .map(|local| (*local, world.get(*local).map(|c| c.clone())))
            .collect::<Vec<_>>();

        query_res.results.insert(id, Some(result));
    }
}

#[derive(Debug, Default)]
struct ChunkQuery {
    queries: HashMap<usize, Vec<IVec3>>,
    results: HashMap<usize, Option<Vec<(IVec3, Option<ChunkKind>)>>>,
    next_id: usize,
}

impl ChunkQuery {
    pub fn request(&mut self, chunks: Vec<IVec3>) -> usize {
        self.next_id += 1;
        self.queries.insert(self.next_id, chunks);
        self.next_id
    }

    pub fn fetch(&mut self, id: usize) -> Option<Vec<(IVec3, Option<ChunkKind>)>> {
        if self.results.contains_key(&id) {
            self.results.get_mut(&id).unwrap().take()
        } else {
            None
        }
    }
}

#[derive(SystemParam)]
struct ChunkSystemQuery<'w, 's> {
    requests: Local<'s, VecDeque<usize>>,
    query: ResMut<'w, ChunkQuery>,
}

impl<'w, 's> ChunkSystemQuery<'w, 's> {
    pub fn query(&mut self, chunks: Vec<IVec3>) {
        let req_id = self.query.request(chunks);
        self.requests.push_back(req_id);
    }

    pub fn fetch(&mut self) -> Option<Vec<(IVec3, Option<ChunkKind>)>> {
        if let Some(next) = self.requests.back() {
            if let Some(res) = self.query.fetch(*next) {
                self.requests.pop_front();
                return Some(res);
            }
        }
        None
    }
}

fn some_query_system(keyboard: Res<Input<KeyCode>>, mut query: ChunkSystemQuery) {
    while let Some(result) = query.fetch() {
        info!("Some!");
        for (local, _) in result {
            dbg!(local);
        }
    }

    if !keyboard.just_pressed(KeyCode::F12) {
        return;
    }

    info!("Sending query");
    query.query(vec![(0, 0, 0).into()]);
}
