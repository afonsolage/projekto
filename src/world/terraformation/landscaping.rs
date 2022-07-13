use bevy::{prelude::*, utils::HashSet};

use crate::world::{query, storage::chunk};

use super::{
    prelude::{BatchChunkCmdRes, WorldRes},
    TerraformationCenter, TerraformationConfig,
};

pub(super) struct LandscapingPlugin;

impl Plugin for LandscapingPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(update_landscape);
    }
}
#[derive(Default)]
struct UpdateLandscapeMeta {
    last_pos: IVec3,
    next_sync: f32,
}

fn update_landscape(
    time: Res<Time>,
    config: Res<TerraformationConfig>,
    world_res: Res<WorldRes>,
    mut meta: Local<UpdateLandscapeMeta>,
    mut batch: ResMut<BatchChunkCmdRes>,
    q: Query<&Transform, With<TerraformationCenter>>,
) {
    let mut _perf = perf_fn!();

    if !world_res.is_ready() {
        return;
    }

    let center = match q.get_single() {
        Ok(t) => chunk::to_local(t.translation),
        Err(_) => return,
    };

    meta.next_sync -= time.delta_seconds();

    if center != meta.last_pos || meta.next_sync < 0.0 {
        perf_scope!(_perf);
        meta.next_sync = 1.0;
        meta.last_pos = center;

        debug!("Updating landscape to center {}", center);

        let radius = IVec3::new(
            config.horizontal_radius as i32,
            config.vertical_radius as i32,
            config.horizontal_radius as i32,
        );

        let begin = center - radius;
        let end = center + radius;

        let visible_range = query::range(begin, end).collect::<HashSet<_>>();
        let existing_chunks = HashSet::from_iter(world_res.list_chunks().into_iter());

        visible_range
            .iter()
            .filter(|&i| !existing_chunks.contains(i))
            .for_each(|v| batch.load(*v));

        existing_chunks
            .iter()
            .filter(|&i| !visible_range.contains(i))
            .for_each(|v| batch.unload(*v));
    }
}
