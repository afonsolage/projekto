use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, platform::collections::HashSet, prelude::*};
use projekto_core::chunk;
use projekto_server::{
    bundle::{ChunkFacesOcclusion, ChunkFacesSoftLight, ChunkKind, ChunkLight},
    set::Landscape,
    WorldServerPlugin,
};

const TICK_EVERY_MILLIS: u64 = 50;

fn main() {
    let mut app = App::new();

    app.add_plugins(LogPlugin::default());

    // TODO: Rework this when plugins dependencies is a thing in bevy
    projekto_server::setup_chunk_asset_loader(&mut app);

    app.add_plugins((
        AssetPlugin::default(),
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(
            TICK_EVERY_MILLIS,
        ))),
        WorldServerPlugin,
    ))
    .add_systems(Update, (set_landscape_once, count_chunk_states))
    .run();
}

fn set_landscape_once(time: Res<Time>, mut commands: Commands, mut done: Local<bool>) {
    if time.elapsed_secs() < 1.0 || *done {
        return;
    }

    commands.insert_resource(Landscape {
        center: IVec2::ZERO,
        radius: 32,
    });

    *done = true;
}

fn count_chunk_states(
    time: Res<Time>,
    mut elapsed: Local<f64>,
    q: Query<(
        &ChunkKind,
        &ChunkLight,
        &ChunkFacesOcclusion,
        &ChunkFacesSoftLight,
    )>,
) {
    *elapsed += time.elapsed().as_secs_f64();

    if *elapsed < 1.0 {
        return;
    }

    *elapsed = 0.0;

    if q.is_empty() {
        info!("No chunks loaded yet!");
        return;
    }

    let begin = std::time::Instant::now();

    let mut data = vec![vec![]; 4];

    for (kind, light, occlusion, soft_light) in q {
        let unique_kinds = chunk::voxels().map(|v| kind.get(v)).collect::<HashSet<_>>();
        let unique_lights = chunk::voxels()
            .map(|v| light.get(v))
            .collect::<HashSet<_>>();
        let unique_occlusions = chunk::voxels()
            .map(|v| occlusion.get(v))
            .collect::<HashSet<_>>();
        let unique_soft_lights = chunk::voxels()
            .map(|v| soft_light.get(v))
            .collect::<HashSet<_>>();

        data[0].push(unique_kinds.len());
        data[1].push(unique_lights.len());
        data[2].push(unique_occlusions.len());
        data[3].push(unique_soft_lights.len());
    }

    let statistics = calc_statistics(&data);
    let duration = (std::time::Instant::now() - begin).as_millis();

    info!("####################");
    info!("####################");
    info!("### Total: {}", data[0].len());
    info!("### Kinds: {:?}", statistics[0]);
    info!("### Lights: {:?}", statistics[1]);
    info!("### Occlusions: {:?}", statistics[2]);
    info!("### Soft Lights: {:?}", statistics[3]);
    info!("####### {duration} #######");
    info!("####################");
}

fn calc_statistics(data: &[Vec<usize>]) -> Vec<(usize, usize, usize)> {
    data.iter()
        .map(|metric| {
            let count = metric.len();
            let over = metric.iter().filter(|v| **v > 255).copied().count();
            let max = metric.iter().copied().max().unwrap_or_default();
            let sum = metric.iter().sum::<usize>();
            (over, (sum / count), max)
        })
        .collect()
}
