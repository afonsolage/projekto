use bevy::{ecs::system::SystemParam, prelude::*, utils::HashMap};

use crate::world::{
    query::{self, RaycastHit},
    storage::{chunk::ChunkKind, voxel},
};

use super::{genesis::BatchChunkCmdRes, WorldRes};

pub(super) struct TerraformingPlugin;

impl Plugin for TerraformingPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CmdChunkUpdate>()
            .add_startup_system_to_stage(super::PipelineStartup::Terraforming, setup_resources)
            .add_system_set_to_stage(
                super::Pipeline::Terraforming,
                SystemSet::new()
                    .with_system(process_update_chunks_system)
                    .with_system(handle_queries_system),
            );
    }
}

fn setup_resources(mut commands: Commands) {
    commands.insert_resource(ChunkQuery::default());
}

#[derive(Clone)]
pub struct CmdChunkUpdate(pub IVec3, pub Vec<(IVec3, voxel::Kind)>);

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
pub struct ChunkQuery {
    requests: HashMap<usize, Vec<IVec3>>,
    results: HashMap<usize, HashMap<IVec3, ChunkKind>>,
    next_id: usize,
}

impl ChunkQuery {
    pub fn request(&mut self, chunks: Vec<IVec3>) -> usize {
        assert!(
            !chunks.is_empty(),
            "At least one chunk local should be provided."
        );
        self.next_id += 1;
        self.requests.insert(self.next_id, chunks);
        self.next_id
    }

    pub fn fetch(&mut self, id: usize) -> Option<HashMap<IVec3, ChunkKind>> {
        self.results.remove(&id)
    }
}

#[derive(SystemParam)]
pub struct ChunkSystemQuery<'w, 's> {
    request: Local<'s, Option<usize>>,
    query: ResMut<'w, ChunkQuery>,
}

impl<'w, 's> ChunkSystemQuery<'w, 's> {
    pub fn is_waiting(&self) -> bool {
        self.request.is_some()
    }

    pub fn query(&mut self, chunks: Vec<IVec3>) {
        let req_id = self.query.request(chunks);
        assert!(
            self.request.replace(req_id).is_none(),
            "Only one query at a time is allowed"
        );
    }

    pub fn fetch(&mut self) -> Option<HashMap<IVec3, ChunkKind>> {
        if let Some(req_id) = *self.request {
            if let Some(res) = self.query.fetch(req_id) {
                self.request.take();
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
        trace!("Setting result for query {} {:?}", id, chunks);

        let result = chunks
            .iter()
            .filter_map(|local| world.get(*local).map(|c| (*local, c.clone())))
            .collect();

        queries_res.results.insert(id, result);
    }
}

pub struct RaycastRequest {
    id: usize,
    origin: Vec3,
    dir: Vec3,
    range: f32,
}

pub struct RaycastResult {
    pub origin: Vec3,
    pub dir: Vec3,
    pub range: f32,
    pub chunks: HashMap<IVec3, ChunkKind>,
}

impl RaycastResult {
    fn new(req: RaycastRequest, chunks: HashMap<IVec3, ChunkKind>) -> Self {
        RaycastResult {
            origin: req.origin,
            dir: req.dir,
            range: req.range,
            chunks,
        }
    }

    pub fn hits(&self) -> Vec<(RaycastHit, Vec<RaycastHit>)> {
        query::raycast(self.origin, self.dir, self.range)
    }
}

#[derive(SystemParam)]
pub struct ChunkSystemRaycast<'w, 's> {
    request: Local<'s, Option<RaycastRequest>>,
    query: ResMut<'w, ChunkQuery>,
}

impl<'w, 's> ChunkSystemRaycast<'w, 's> {
    pub fn is_waiting(&self) -> bool {
        self.request.is_some()
    }

    pub fn fetch(&mut self) -> Option<RaycastResult> {
        if let Some(ref req) = *self.request {
            if let Some(res) = self.query.fetch(req.id) {
                let req = self.request.take().unwrap();
                return Some(RaycastResult::new(req, res));
            }
        }
        None
    }

    pub fn raycast(&mut self, origin: Vec3, dir: Vec3, range: f32) {
        let chunks = query::raycast(origin, dir, range)
            .into_iter()
            .map(|(r, _)| r.local)
            .collect();

        let id = self.query.request(chunks);
        self.request.replace(RaycastRequest {
            id,
            origin,
            dir,
            range,
        });
    }
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

        query.results.insert(
            req_id,
            vec![((0, 0, 0).into(), ChunkKind::default())]
                .into_iter()
                .collect(),
        );

        let fetch = query.fetch(req_id);

        assert!(fetch.is_some());

        let res = fetch.unwrap();

        assert!(res.contains_key(&(0, 0, 0).into()));
        assert!(res.get(&(0, 0, 0).into()).is_some());

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
