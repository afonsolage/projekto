use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
    str::FromStr,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bevy::{app::ScheduleRunnerPlugin, prelude::*, time::TimeUpdateStrategy};
use projekto_archive::ArchiveServer;
use projekto_core::coords::Chunk;
use projekto_server::{
    ChunkAsset, DirtyChunks, WorldServerPlugin, WorldSet,
    bundle::{ChunkKind, ChunkLocal, ChunkLod, ChunkMap, ChunkVertex},
    set::next_lod,
    set::{LOD_FULL_RADIUS, Landscape, MeshTasks, TerrainStats},
};

const DEFAULT_RADIUS: u8 = 18;
const DEFAULT_WALK_CHUNKS: i32 = 24;
const DEFAULT_SPEED: f32 = 10.0;
const DEFAULT_TICK_MS: u64 = 50;
const DEFAULT_SETTLE_TIMEOUT_SECS: u64 = 180;
const DEFAULT_MAX_LOAD_SECS: f64 = 30.0;
const DEFAULT_MAX_READY_SECS: f64 = 30.0;

#[test]
#[ignore = "streaming benchmark; run with: cargo test -p projekto_server --test terrain_walk -- --ignored --nocapture"]
fn terrain_follows_a_steady_player_walk() {
    let config = WalkConfig::from_env();
    let mut app = build_app(&config);
    let report = run_walk(&mut app, &config);

    drop(app);
    let _ = fs::remove_dir_all(&config.archive_dir);

    report.print(&config);
    report.assert(&config);
}

struct WalkConfig {
    radius: u8,
    walk_chunks: i32,
    speed: f32,
    tick: Duration,
    settle_timeout: Duration,
    max_load_secs: f64,
    max_ready_secs: f64,
    max_wrong_lod_ticks: u32,
    max_walk_update_ms: f64,
    max_walk_update_p95_ms: f64,
    near_radius: u8,
    max_near_missing: usize,
    archive_dir: PathBuf,
    archive_path: String,
}

impl WalkConfig {
    fn from_env() -> Self {
        let radius = env_parse("PROJEKTO_TERRAIN_TEST_RADIUS", DEFAULT_RADIUS);
        let walk_chunks = env_parse("PROJEKTO_TERRAIN_TEST_WALK_CHUNKS", DEFAULT_WALK_CHUNKS);
        let speed = env_parse("PROJEKTO_TERRAIN_TEST_SPEED", DEFAULT_SPEED);
        let tick_ms = env_parse("PROJEKTO_TERRAIN_TEST_TICK_MS", DEFAULT_TICK_MS);
        let near_radius = env_parse("PROJEKTO_TERRAIN_TEST_NEAR_RADIUS", 8u8);

        assert!(
            radius as i32 > LOD_FULL_RADIUS,
            "radius must exceed LOD_FULL_RADIUS ({LOD_FULL_RADIUS}) so the walk crosses LOD boundaries"
        );
        assert!(walk_chunks > 0, "walk distance must be positive");
        assert!(speed > 0.0, "walk speed must be positive");
        assert!(tick_ms > 0, "tick duration must be positive");
        assert!(
            near_radius <= radius,
            "near radius must not exceed the landscape radius"
        );

        let settle_timeout_secs = env_parse(
            "PROJEKTO_TERRAIN_TEST_SETTLE_TIMEOUT_SECS",
            DEFAULT_SETTLE_TIMEOUT_SECS,
        );
        let max_load_secs = env_parse("PROJEKTO_TERRAIN_TEST_MAX_LOAD_SECS", DEFAULT_MAX_LOAD_SECS);
        let max_ready_secs = env_parse(
            "PROJEKTO_TERRAIN_TEST_MAX_READY_SECS",
            DEFAULT_MAX_READY_SECS,
        );
        let max_wrong_lod_ticks = env_parse("PROJEKTO_TERRAIN_TEST_MAX_WRONG_LOD_TICKS", 8u32);
        let max_walk_update_ms = env_parse("PROJEKTO_TERRAIN_TEST_MAX_WALK_UPDATE_MS", 250.0f64);
        let max_walk_update_p95_ms =
            env_parse("PROJEKTO_TERRAIN_TEST_MAX_WALK_UPDATE_P95_MS", 25.0f64);
        let max_near_missing = env_parse("PROJEKTO_TERRAIN_TEST_MAX_NEAR_MISSING", 0usize);

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let archive_dir = env::temp_dir().join(format!(
            "projekto-terrain-walk-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&archive_dir).expect("create temporary archive directory");
        let archive_path = format!("{}/", archive_dir.display());

        Self {
            radius,
            walk_chunks,
            speed,
            tick: Duration::from_millis(tick_ms),
            settle_timeout: Duration::from_secs(settle_timeout_secs),
            max_load_secs,
            max_ready_secs,
            max_wrong_lod_ticks,
            max_walk_update_ms,
            max_walk_update_p95_ms,
            near_radius,
            max_near_missing,
            archive_dir,
            archive_path,
        }
    }
}

fn env_parse<T: FromStr>(name: &str, default: T) -> T {
    match env::var(name) {
        Ok(value) => value
            .parse()
            .unwrap_or_else(|_| panic!("invalid value for {name}: {value}")),
        Err(env::VarError::NotPresent) => default,
        Err(error) => panic!("cannot read {name}: {error}"),
    }
}

#[derive(Resource)]
struct TestSetup {
    archive_path: String,
    radius: u8,
}

fn build_app(config: &WalkConfig) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins
            .set(ScheduleRunnerPlugin::run_once())
            .set(TaskPoolPlugin {
                task_pool_options: projekto_server::server_task_pool_options(),
            }),
        AssetPlugin::default(),
        WorldServerPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(config.tick))
    .insert_resource(TestSetup {
        archive_path: config.archive_path.clone(),
        radius: config.radius,
    })
    .init_resource::<MeshingRan>()
    .add_systems(Startup, setup_world)
    .add_systems(Update, note_meshing.in_set(WorldSet::Meshing));

    app
}

fn setup_world(mut commands: Commands, setup: Res<TestSetup>) {
    commands.insert_resource(ArchiveServer::<ChunkAsset>::new(&setup.archive_path));
    commands.insert_resource(Landscape {
        center: IVec2::ZERO,
        radius: setup.radius,
    });
}

#[derive(Resource, Default)]
struct MeshingRan(bool);

fn note_meshing(mut ran: ResMut<MeshingRan>) {
    ran.0 = true;
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Initial,
    Walk,
    Final,
}

impl Phase {
    fn index(self) -> usize {
        match self {
            Phase::Initial => 0,
            Phase::Walk => 1,
            Phase::Final => 2,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Phase::Initial => "initial",
            Phase::Walk => "walk",
            Phase::Final => "final",
        }
    }
}

const PHASES: [Phase; 3] = [Phase::Initial, Phase::Walk, Phase::Final];

#[derive(Default)]
struct PhaseStats {
    load_seconds: Vec<f64>,
    ready_seconds: Vec<f64>,
    update_seconds: Vec<f64>,
    meshing_update_seconds: Vec<f64>,
}

struct Metrics {
    phases: [PhaseStats; 3],
    lod_events: Vec<(u8, u8)>,
    loaded_total: usize,
    unloaded_total: usize,
    unloaded_unready: usize,
    max_extra: usize,
    max_missing_walk: usize,
    max_missing_near_walk: usize,
    min_ready_ratio: f64,
    min_near_ready_ratio: f64,
    min_visible_ratio: f64,
    min_near_visible_ratio: f64,
    max_holes: usize,
    max_wrong_lod_streak: u32,
    max_transitions_per_chunk: u32,
    full_vertices_sum: usize,
    full_vertices_count: usize,
    coarse_vertices_sum: usize,
    coarse_vertices_count: usize,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            phases: std::array::from_fn(|_| PhaseStats::default()),
            lod_events: Vec::new(),
            loaded_total: 0,
            unloaded_total: 0,
            unloaded_unready: 0,
            max_extra: 0,
            max_missing_walk: 0,
            max_missing_near_walk: 0,
            min_ready_ratio: 1.0,
            min_near_ready_ratio: 1.0,
            min_visible_ratio: 1.0,
            min_near_visible_ratio: 1.0,
            max_holes: 0,
            max_wrong_lod_streak: 0,
            max_transitions_per_chunk: 0,
            full_vertices_sum: 0,
            full_vertices_count: 0,
            coarse_vertices_sum: 0,
            coarse_vertices_count: 0,
        }
    }
}

#[derive(Default)]
struct ChunkRecord {
    desired_since: Option<(u64, Instant)>,
    desired_phase: Phase,
    loaded: bool,
    loaded_at: Option<(u64, Instant)>,
    loaded_phase: Phase,
    ready_at: Option<(u64, Instant)>,
    last_lod: Option<u8>,
    transitions: u32,
    wrong_lod_streak: u32,
}

struct Harness {
    radius: u8,
    near_radius: u8,
    records: HashMap<Chunk, ChunkRecord>,
    metrics: Metrics,
    last_meshing_tick: Option<u64>,
}

impl Harness {
    fn new(radius: u8, near_radius: u8) -> Self {
        Self {
            radius,
            near_radius,
            records: HashMap::new(),
            metrics: Metrics::default(),
            last_meshing_tick: None,
        }
    }

    fn run_tick(&mut self, app: &mut App, tick: u64, phase: Phase) -> Observation {
        app.world_mut().resource_mut::<MeshingRan>().0 = false;

        let started = Instant::now();
        app.update();
        let elapsed = started.elapsed();
        let meshing_ran = app.world().resource::<MeshingRan>().0;

        if meshing_ran {
            self.last_meshing_tick = Some(tick);
        }

        {
            let phase_stats = &mut self.metrics.phases[phase.index()];
            phase_stats.update_seconds.push(elapsed.as_secs_f64());
            if meshing_ran {
                phase_stats
                    .meshing_update_seconds
                    .push(elapsed.as_secs_f64());
            }
        }

        self.observe(app, tick, Instant::now(), phase)
    }

    fn observe(&mut self, app: &mut App, tick: u64, now: Instant, phase: Phase) -> Observation {
        let world = app.world_mut();
        let center = world.resource::<Landscape>().center;
        let expected: HashSet<Chunk> = expected_chunks(center, self.radius).into_iter().collect();
        let loaded: HashSet<Chunk> = world.resource::<ChunkMap>().0.keys().copied().collect();
        let dirty: HashSet<Chunk> = world.resource::<DirtyChunks>().0.iter().copied().collect();
        let pending: HashSet<Chunk> = world.resource::<MeshTasks>().pending().collect();

        let extra = loaded.difference(&expected).count();
        let missing = expected.difference(&loaded).count();
        let near_expected: HashSet<Chunk> = expected_chunks(center, self.near_radius)
            .into_iter()
            .collect();
        let near_total = near_expected.len();

        self.metrics.max_extra = self.metrics.max_extra.max(extra);

        for &chunk in expected.difference(&loaded) {
            let record = self.records.entry(chunk).or_default();
            if record.desired_since.is_none() {
                record.desired_since = Some((tick, now));
                record.desired_phase = phase;
            }
        }

        let unloaded = self
            .records
            .iter()
            .filter(|(chunk, record)| record.loaded && !loaded.contains(chunk))
            .map(|(chunk, _)| *chunk)
            .collect::<Vec<_>>();

        for chunk in unloaded {
            let record = self
                .records
                .get_mut(&chunk)
                .expect("unloaded chunk must have a record");

            record.loaded = false;
            if record.ready_at.is_none() {
                self.metrics.unloaded_unready += 1;
            }

            record.loaded_at = None;
            record.ready_at = None;
            record.last_lod = None;
            record.wrong_lod_streak = 0;
            if !expected.contains(&chunk) {
                record.desired_since = None;
            }

            self.metrics.unloaded_total += 1;
        }

        let mut holes = 0;
        let mut ready_count = 0;
        let mut near_ready_count = 0;
        let mut visible_count = 0;
        let mut near_visible_count = 0;
        let mut query = world.query::<(&ChunkLocal, &ChunkKind, &ChunkVertex, Option<&ChunkLod>)>();

        for (local, kind, vertex, lod) in query.iter(world) {
            let chunk = local.0;
            if !expected.contains(&chunk) {
                continue;
            }

            let actual = lod.map(|lod| lod.0).unwrap_or(0);
            let is_dirty = dirty.contains(&chunk);
            let record = self.records.entry(chunk).or_default();
            let desired = next_lod(record.last_lod.unwrap_or(actual), chunk, center);

            if !record.loaded {
                record.loaded = true;
                record.loaded_at = Some((tick, now));
                record.loaded_phase = phase;

                if let Some((_, since)) = record.desired_since.take() {
                    let seconds = now.saturating_duration_since(since).as_secs_f64();
                    self.metrics.phases[record.desired_phase.index()]
                        .load_seconds
                        .push(seconds);
                }

                self.metrics.loaded_total += 1;
            }

            let loaded_tick = record.loaded_at.map(|(tick, _)| tick).unwrap_or(tick);
            let meshed_once = self
                .last_meshing_tick
                .is_some_and(|meshing| meshing >= loaded_tick);
            let ready = actual == desired
                && !is_dirty
                && !pending.contains(&chunk)
                && meshed_once
                && (actual != 0 || !vertex.0.is_empty() || kind.0.all(|kind| kind.is_none()));

            if ready {
                ready_count += 1;
                if near_expected.contains(&chunk) {
                    near_ready_count += 1;
                }
            }

            if !vertex.0.is_empty() {
                visible_count += 1;
                if near_expected.contains(&chunk) {
                    near_visible_count += 1;
                }
            }

            let first_ready = record.ready_at.is_none() && ready;
            if first_ready {
                record.ready_at = Some((tick, now));
                if let Some((_, loaded_at)) = record.loaded_at {
                    let seconds = now.saturating_duration_since(loaded_at).as_secs_f64();
                    self.metrics.phases[record.loaded_phase.index()]
                        .ready_seconds
                        .push(seconds);
                }
            }

            if let Some(previous) = record.last_lod
                && previous != actual
            {
                self.metrics.lod_events.push((previous, actual));
                record.transitions += 1;
                self.metrics.max_transitions_per_chunk = self
                    .metrics
                    .max_transitions_per_chunk
                    .max(record.transitions);
            }
            record.last_lod = Some(actual);

            if actual == desired {
                record.wrong_lod_streak = 0;
            } else {
                record.wrong_lod_streak += 1;
                self.metrics.max_wrong_lod_streak = self
                    .metrics
                    .max_wrong_lod_streak
                    .max(record.wrong_lod_streak);
            }

            if first_ready && !vertex.0.is_empty() && has_all_horizontal_neighbors(&loaded, chunk) {
                if actual == 0 {
                    self.metrics.full_vertices_sum += vertex.0.len();
                    self.metrics.full_vertices_count += 1;
                } else {
                    self.metrics.coarse_vertices_sum += vertex.0.len();
                    self.metrics.coarse_vertices_count += 1;
                }
            }

            let is_hole = actual == 0
                && vertex.0.is_empty()
                && !kind.0.all(|kind| kind.is_none())
                && !is_dirty
                && !pending.contains(&chunk)
                && meshed_once;

            if is_hole {
                holes += 1;
            }
        }

        self.metrics.max_holes = self.metrics.max_holes.max(holes);

        let near_missing = near_total.saturating_sub(near_ready_count);

        if phase == Phase::Walk {
            self.metrics.max_missing_walk = self.metrics.max_missing_walk.max(missing);
            self.metrics.max_missing_near_walk =
                self.metrics.max_missing_near_walk.max(near_missing);
            self.metrics.min_ready_ratio = self
                .metrics
                .min_ready_ratio
                .min(ready_count as f64 / expected.len() as f64);
            self.metrics.min_near_ready_ratio = self
                .metrics
                .min_near_ready_ratio
                .min(near_ready_count as f64 / near_total as f64);
            self.metrics.min_visible_ratio = self
                .metrics
                .min_visible_ratio
                .min(visible_count as f64 / expected.len() as f64);
            self.metrics.min_near_visible_ratio = self
                .metrics
                .min_near_visible_ratio
                .min(near_visible_count as f64 / near_total as f64);
        }

        Observation {
            expected: expected.len(),
            loaded: loaded.len(),
            missing,
            extra,
            holes,
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
struct Observation {
    expected: usize,
    loaded: usize,
    missing: usize,
    extra: usize,
    holes: usize,
}

struct Report {
    initial: Observation,
    final_obs: Observation,
    initial_settled: bool,
    final_settled: bool,
    initial_time: Duration,
    walk_time: Duration,
    final_time: Duration,
    tick_count: u64,
    walk_ticks: u64,
    metrics: Metrics,
    stats: TerrainStats,
}

fn run_walk(app: &mut App, config: &WalkConfig) -> Report {
    let mut harness = Harness::new(config.radius, config.near_radius);
    let mut deadline = Instant::now();
    let mut tick = 0u64;

    let initial_start = Instant::now();
    let (initial, initial_settled) = loop {
        pace(&mut deadline, config.tick);
        let observation = harness.run_tick(app, tick, Phase::Initial);
        tick += 1;

        if is_settled(app, config.radius) {
            break (observation, true);
        }
        if initial_start.elapsed() > config.settle_timeout {
            break (observation, false);
        }
    };
    let initial_time = initial_start.elapsed();

    let walk_start = Instant::now();
    let walk_ticks = (config.walk_chunks as f64 * Chunk::X_AXIS_SIZE as f64
        / (config.speed as f64 * config.tick.as_secs_f64()))
    .ceil() as u64;

    for step in 0..walk_ticks {
        pace(&mut deadline, config.tick);

        let player_x = (step + 1) as f64 * config.speed as f64 * config.tick.as_secs_f64();
        set_landscape_center(app, Chunk::from(Vec3::new(player_x as f32, 0.0, 0.0)));
        harness.run_tick(app, tick, Phase::Walk);

        tick += 1;
    }

    set_landscape_center(
        app,
        Chunk::from(Vec3::new(
            (config.walk_chunks * Chunk::X_AXIS_SIZE as i32) as f32,
            0.0,
            0.0,
        )),
    );
    let walk_time = walk_start.elapsed();

    let final_start = Instant::now();
    let (final_obs, final_settled) = loop {
        pace(&mut deadline, config.tick);
        let observation = harness.run_tick(app, tick, Phase::Final);
        tick += 1;

        if is_settled(app, config.radius) {
            break (observation, true);
        }
        if final_start.elapsed() > config.settle_timeout {
            break (observation, false);
        }
    };
    let final_time = final_start.elapsed();
    let stats = *app.world().resource::<TerrainStats>();

    Report {
        initial,
        final_obs,
        initial_settled,
        final_settled,
        initial_time,
        walk_time,
        final_time,
        tick_count: tick,
        walk_ticks,
        metrics: harness.metrics,
        stats,
    }
}

impl Report {
    fn print(&self, config: &WalkConfig) {
        println!("=== terrain walk report ===");
        println!(
            "config: radius={} walk_chunks={} speed={:.1} tick={:?}",
            config.radius, config.walk_chunks, config.speed, config.tick
        );
        println!(
            "initial: settled={} time={:.2}s expected={} loaded={} holes={}",
            self.initial_settled,
            self.initial_time.as_secs_f64(),
            self.initial.expected,
            self.initial.loaded,
            self.initial.holes
        );
        println!(
            "walk: ticks={} time={:.2}s max_missing={} max_missing_near={}",
            self.walk_ticks,
            self.walk_time.as_secs_f64(),
            self.metrics.max_missing_walk,
            self.metrics.max_missing_near_walk
        );
        println!(
            "visibility: min_visible={:.1}% min_ready={:.1}% min_near_visible={:.1}% min_near_ready={:.1}%",
            self.metrics.min_visible_ratio * 100.0,
            self.metrics.min_ready_ratio * 100.0,
            self.metrics.min_near_visible_ratio * 100.0,
            self.metrics.min_near_ready_ratio * 100.0
        );
        let mesh_tasks = self.stats.meshed_full + self.stats.meshed_coarse;
        let average_mesh_ms = if mesh_tasks == 0 {
            0.0
        } else {
            self.stats.mesh_nanos as f64 / mesh_tasks as f64 / 1_000_000.0
        };
        println!(
            "terrain stats: generated={} archive_loaded={} unloaded={} saved={} meshed_full={} meshed_coarse={} avg_mesh={average_mesh_ms:.1}ms",
            self.stats.generated,
            self.stats.archive_loaded,
            self.stats.unloaded,
            self.stats.saved,
            self.stats.meshed_full,
            self.stats.meshed_coarse
        );
        println!(
            "final: settled={} time={:.2}s expected={} loaded={} missing={} extra={} holes={}",
            self.final_settled,
            self.final_time.as_secs_f64(),
            self.final_obs.expected,
            self.final_obs.loaded,
            self.final_obs.missing,
            self.final_obs.extra,
            self.final_obs.holes
        );
        println!(
            "streaming: ticks={} loaded={} unloaded={} unloaded_before_ready={}",
            self.tick_count,
            self.metrics.loaded_total,
            self.metrics.unloaded_total,
            self.metrics.unloaded_unready
        );
        let total_time = self.initial_time + self.walk_time + self.final_time;
        println!(
            "throughput: {:.1} chunks/s",
            self.metrics.loaded_total as f64 / total_time.as_secs_f64()
        );
        for phase in PHASES {
            let stats = &self.metrics.phases[phase.index()];
            println!(
                "update {}: {}",
                phase.name(),
                format_stats(&stats.update_seconds, 1000.0, "ms")
            );
        }
        println!(
            "update all: {}",
            format_stats(
                &self
                    .metrics
                    .phases
                    .iter()
                    .flat_map(|phase| phase.update_seconds.iter().copied())
                    .collect::<Vec<_>>(),
                1000.0,
                "ms"
            )
        );
        println!(
            "update meshing: {}",
            format_stats(
                &self
                    .metrics
                    .phases
                    .iter()
                    .flat_map(|phase| phase.meshing_update_seconds.iter().copied())
                    .collect::<Vec<_>>(),
                1000.0,
                "ms"
            )
        );

        for phase in PHASES {
            let stats = &self.metrics.phases[phase.index()];
            println!(
                "load latency {}: {}",
                phase.name(),
                format_stats(&stats.load_seconds, 1.0, "s")
            );
            println!(
                "ready latency {}: {}",
                phase.name(),
                format_stats(&stats.ready_seconds, 1.0, "s")
            );
        }

        let to_coarse = self
            .metrics
            .lod_events
            .iter()
            .filter(|(from, to)| *from == 0 && *to > 0)
            .count();
        let to_full = self
            .metrics
            .lod_events
            .iter()
            .filter(|(from, to)| *from > 0 && *to == 0)
            .count();

        println!(
            "lod: transitions={} full_to_coarse={} coarse_to_full={} max_per_chunk={}",
            self.metrics.lod_events.len(),
            to_coarse,
            to_full,
            self.metrics.max_transitions_per_chunk
        );
        println!(
            "max wrong-lod streak: {} ticks",
            self.metrics.max_wrong_lod_streak
        );

        if self.metrics.full_vertices_count > 0 && self.metrics.coarse_vertices_count > 0 {
            let full_average =
                self.metrics.full_vertices_sum as f64 / self.metrics.full_vertices_count as f64;
            let coarse_average =
                self.metrics.coarse_vertices_sum as f64 / self.metrics.coarse_vertices_count as f64;
            println!(
                "mesh vertices (interior chunks): full avg={full_average:.1} (n={}) coarse avg={coarse_average:.1} (n={}) ratio={:.2}",
                self.metrics.full_vertices_count,
                self.metrics.coarse_vertices_count,
                coarse_average / full_average
            );
        }
    }

    fn assert(&self, config: &WalkConfig) {
        assert!(
            self.initial_settled,
            "initial landscape did not settle within {:?}",
            config.settle_timeout
        );
        assert!(
            self.final_settled,
            "final landscape did not settle within {:?}",
            config.settle_timeout
        );
        assert_eq!(
            self.final_obs.missing, 0,
            "final landscape is missing chunks"
        );
        assert_eq!(self.final_obs.extra, 0, "final landscape has extra chunks");
        assert_eq!(
            self.final_obs.loaded, self.final_obs.expected,
            "final landscape chunk count does not match the desired area"
        );
        assert_eq!(
            self.final_obs.holes, 0,
            "final landscape has full-detail chunks with empty meshes over solid terrain"
        );
        assert_eq!(
            self.metrics.max_extra, 0,
            "chunks outside the landscape stayed loaded"
        );
        assert!(
            self.metrics.max_missing_near_walk <= config.max_near_missing,
            "the {}x{} near-player area had {} missing chunks during the walk, limit is {}",
            config.near_radius as usize * 2 + 1,
            config.near_radius as usize * 2 + 1,
            self.metrics.max_missing_near_walk,
            config.max_near_missing
        );
        assert_eq!(
            self.metrics.max_holes, 0,
            "full-detail chunks produced empty meshes over solid terrain"
        );
        assert_eq!(
            self.metrics.unloaded_unready, 0,
            "chunks were unloaded before becoming ready"
        );
        assert!(
            self.metrics.max_wrong_lod_streak <= config.max_wrong_lod_ticks,
            "a chunk stayed at the wrong LOD for {} ticks, limit is {}",
            self.metrics.max_wrong_lod_streak,
            config.max_wrong_lod_ticks
        );
        assert!(
            self.metrics.max_transitions_per_chunk <= 3,
            "a chunk changed LOD {} times during a straight walk, expected at most 3",
            self.metrics.max_transitions_per_chunk
        );
        assert!(
            !self.metrics.lod_events.is_empty(),
            "the walk never crossed a LOD boundary"
        );
        assert!(
            self.metrics.min_visible_ratio >= 0.90,
            "visibility ratio fell to {:.1}%, below 90%",
            self.metrics.min_visible_ratio * 100.0
        );
        assert!(
            self.metrics.min_near_visible_ratio >= 0.90,
            "near visibility ratio fell to {:.1}%, below 90%",
            self.metrics.min_near_visible_ratio * 100.0
        );
        assert!(
            self.metrics.min_ready_ratio >= 0.80,
            "ready ratio fell to {:.1}%, below 80%",
            self.metrics.min_ready_ratio * 100.0
        );
        assert!(self.stats.generated > 0, "no chunks were generated");
        assert!(self.stats.unloaded > 0, "no chunks were unloaded");
        assert!(self.stats.saved > 0, "no chunks were saved");
        assert!(
            self.stats.meshed_full > 0,
            "no full-detail meshes were built"
        );
        assert!(self.stats.meshed_coarse > 0, "no coarse meshes were built");

        let walk_updates = &self.metrics.phases[Phase::Walk.index()].update_seconds;
        let max_walk_update = walk_updates.iter().copied().fold(0.0, f64::max) * 1000.0;
        assert!(
            max_walk_update <= config.max_walk_update_ms,
            "slowest walk update took {max_walk_update:.1}ms, limit is {:.1}ms",
            config.max_walk_update_ms
        );

        if !walk_updates.is_empty() {
            let mut sorted = walk_updates.clone();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let p95 = percentile(&sorted, 0.95) * 1000.0;
            assert!(
                p95 <= config.max_walk_update_p95_ms,
                "walk update p95 was {p95:.1}ms, limit is {:.1}ms",
                config.max_walk_update_p95_ms
            );
        }

        let max_load = self
            .metrics
            .phases
            .iter()
            .flat_map(|phase| phase.load_seconds.iter().copied())
            .fold(0.0, f64::max);
        assert!(
            max_load <= config.max_load_secs,
            "slowest chunk took {max_load:.1}s from desired to loaded, limit is {:.1}s",
            config.max_load_secs
        );

        let max_ready = self
            .metrics
            .phases
            .iter()
            .flat_map(|phase| phase.ready_seconds.iter().copied())
            .fold(0.0, f64::max);
        assert!(
            max_ready <= config.max_ready_secs,
            "slowest chunk took {max_ready:.1}s from loaded to ready, limit is {:.1}s",
            config.max_ready_secs
        );
    }
}

fn pace(deadline: &mut Instant, tick: Duration) {
    *deadline += tick;
    let now = Instant::now();

    if *deadline > now {
        thread::sleep(*deadline - now);
    } else {
        *deadline = now;
    }
}

fn set_landscape_center(app: &mut App, chunk: Chunk) {
    let center = IVec2::from(chunk);
    let mut landscape = app.world_mut().resource_mut::<Landscape>();

    if landscape.center != center {
        landscape.center = center;
    }
}

fn expected_chunks(center: IVec2, radius: u8) -> Vec<Chunk> {
    let radius = radius as i32;
    let diameter = radius * 2 + 1;
    let mut chunks = Vec::with_capacity((diameter * diameter) as usize);

    for x in -radius..=radius {
        for z in -radius..=radius {
            chunks.push(Chunk::new(center.x + x, center.y + z));
        }
    }

    chunks
}

fn has_all_horizontal_neighbors(loaded: &HashSet<Chunk>, chunk: Chunk) -> bool {
    loaded.contains(&Chunk::new(chunk.x - 1, chunk.z))
        && loaded.contains(&Chunk::new(chunk.x + 1, chunk.z))
        && loaded.contains(&Chunk::new(chunk.x, chunk.z - 1))
        && loaded.contains(&Chunk::new(chunk.x, chunk.z + 1))
}

fn is_settled(app: &mut App, radius: u8) -> bool {
    let world = app.world_mut();
    let center = world.resource::<Landscape>().center;
    let expected: HashSet<Chunk> = expected_chunks(center, radius).into_iter().collect();
    let loaded: HashSet<Chunk> = world.resource::<ChunkMap>().0.keys().copied().collect();

    if loaded != expected {
        return false;
    }

    let dirty: HashSet<Chunk> = world.resource::<DirtyChunks>().0.iter().copied().collect();
    let pending: HashSet<Chunk> = world.resource::<MeshTasks>().pending().collect();
    let mut query = world.query::<(&ChunkLocal, &ChunkKind, &ChunkVertex, Option<&ChunkLod>)>();

    for (local, kind, vertex, lod) in query.iter(world) {
        let chunk = local.0;
        if !expected.contains(&chunk) {
            continue;
        }

        let actual = lod.map(|lod| lod.0).unwrap_or(0);
        let desired = next_lod(actual, chunk, center);

        if actual != desired || dirty.contains(&chunk) || pending.contains(&chunk) {
            return false;
        }

        if actual == 0 && vertex.0.is_empty() && !kind.0.all(|kind| kind.is_none()) {
            return false;
        }
    }

    true
}

fn format_stats(values: &[f64], multiplier: f64, unit: &str) -> String {
    if values.is_empty() {
        return "n=0".to_string();
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let p50 = percentile(&sorted, 0.50) * multiplier;
    let p95 = percentile(&sorted, 0.95) * multiplier;
    let max = sorted.last().copied().unwrap_or_default() * multiplier;

    format!(
        "n={} p50={p50:.1}{unit} p95={p95:.1}{unit} max={max:.1}{unit}",
        sorted.len()
    )
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}
