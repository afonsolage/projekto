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
                    .with_system(test_query_system)
                    .with_system(handle_queries_system),
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

#[derive(Debug, Default)]
struct ChunkQuery {
    requests: HashMap<usize, Vec<IVec3>>,
    results: HashMap<usize, Vec<(IVec3, Option<ChunkKind>)>>,
    next_id: usize,
}

impl ChunkQuery {
    pub fn request(&mut self, chunks: Vec<IVec3>) -> usize {
        assert!(
            chunks.len() > 0,
            "At least one chunk local should be provided."
        );
        self.next_id += 1;
        self.requests.insert(self.next_id, chunks);
        self.next_id
    }

    pub fn fetch(&mut self, id: usize) -> Option<Vec<(IVec3, Option<ChunkKind>)>> {
        self.results.remove(&id)
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

fn handle_queries_system(world: Res<WorldRes>, mut queries_res: ResMut<ChunkQuery>) {
    if !world.is_ready() {
        return;
    }

    let requests = queries_res.requests.drain().collect::<Vec<_>>();

    for (id, chunks) in requests {
        info!("Setting result for query {} {:?}", id, chunks);

        let result = chunks
            .iter()
            .map(|local| (*local, world.get(*local).map(|c| c.clone())))
            .collect::<Vec<_>>();

        queries_res.results.insert(id, result);
    }
}

fn test_query_system(keyboard: Res<Input<KeyCode>>, mut query: ChunkSystemQuery) {
    while let Some(result) = query.fetch() {
        info!("Some!");
        for (local, _) in result {
            dbg!(local);
        }
    }

    if !keyboard.just_pressed(KeyCode::F12) {
        return;
    }

    // TODO: Add raycast, add tests and refactor current system. Should I rename this module? Maybe querying?

    info!("Sending query");
    query.query(vec![(0, 0, 0).into()]);
}

#[cfg(test)]
mod tests {
    use bevy::utils::HashSet;

    use super::*;

    #[test]
    #[should_panic]
    fn chunk_query_empty() {
        let mut query = ChunkQuery::default();

        let _ = query.request(vec![]);
    }

    #[test]
    fn chunk_query() {
        let mut query = ChunkQuery::default();
        let req_id = query.request(vec![(0, 0, 0).into()]);

        assert!(req_id > 0, "A non-zero valid unique ID should be returned");

        assert_eq!(
            query.fetch(req_id),
            None,
            "A query fetch without result should return None"
        );

        assert_eq!(
            query.fetch(12345),
            None,
            "A non-existing request id should return None"
        );

        query
            .results
            .insert(req_id, vec![((0, 0, 0).into(), Some(ChunkKind::default()))]);

        let fetch = query.fetch(req_id);

        assert!(fetch.is_some());

        let res = fetch.unwrap();

        assert_eq!(res[0].0, (0, 0, 0).into());
        assert!(res[0].1.is_some());

        assert_eq!(
            query.fetch(req_id),
            None,
            "A second fetch on same request id should return None"
        );

        let mut ids = HashSet::default();
        for _ in 0..1000 {
            let id = query.request(vec![(0, 1, 2).into()]);

            assert!(!ids.contains(&id), "Request id should always be unique");
            ids.insert(id);
        }
    }
}
