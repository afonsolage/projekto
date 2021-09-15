use bevy::{app::AppExit, prelude::*};

#[cfg(perf_counter)]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup_hold_est_to_exit)
            .add_system(hold_esc_to_exit);

        #[cfg(perf_counter)]
        app.add_system(print_perf_counter)
            .init_resource::<PerfCounterRes>();
    }
}

const ESC_HOLD_TIMEOUT: f32 = 0.2;
struct EscHolding(f32);

fn setup_hold_est_to_exit(mut commands: Commands) {
    commands.insert_resource(EscHolding(0.0));
}

fn hold_esc_to_exit(
    mut esc_holding: ResMut<EscHolding>,
    time: Res<Time>,
    input_keys: Res<Input<KeyCode>>,
    mut exit_writer: EventWriter<AppExit>,
) {
    if input_keys.pressed(KeyCode::Escape) {
        esc_holding.0 += time.delta_seconds();

        if esc_holding.0 >= ESC_HOLD_TIMEOUT {
            info!("Exiting app due to ESC holding...");
            exit_writer.send(AppExit);
        }
    } else {
        esc_holding.0 = 0.0;
    }
}

#[cfg(perf_counter)]
mod perf {

    fn print_perf_counter(input_keys: Res<Input<KeyCode>>, perf_counter: Res<PerfCounterRes>) {
        if input_keys.just_pressed(KeyCode::F12) {
            let guard = perf_counter.lock().unwrap();

            let mut output = String::default();
            let mut counters = guard.0.values().collect::<Vec<_>>();
            counters.sort_by(|a, b| (b.elapsed / b.counter).cmp(&(a.elapsed / a.counter)));
            for p in counters {
                output += format!("{}\n", p).as_str();
            }

            info!("Performance Counter: \n{}", output);
        }
    }

    pub type PerfCounterRes = Arc<Mutex<PerfCounterMap>>;

    #[derive(Default)]
    pub struct PerfCounterMap(HashMap<String, PerfCounter>);

    impl PerfCounterMap {
        pub fn add(&mut self, counter: PerfCounter) {
            if counter.is_empty() {
                return;
            }

            let entry = self.0.entry(counter.name.clone()).or_default();
            entry.add(counter);
        }
    }

    pub struct PerfCounterGuard<'a>(&'a mut PerfCounter, std::time::Instant);

    impl<'a> PerfCounterGuard<'a> {
        pub fn new(perf_counter: &'a mut PerfCounter) -> Self {
            Self(perf_counter, std::time::Instant::now())
        }
    }

    impl<'a> Drop for PerfCounterGuard<'a> {
        fn drop(&mut self) {
            let end = self.1.elapsed();
            let duration = end.as_micros() as u64;

            if duration == 0 {
                return;
            }

            self.0.counter += 1u64;
            self.0.elapsed += duration;

            if duration < self.0.min {
                self.0.min = duration;
            }
            if duration > self.0.max {
                self.0.max = duration;
            }
        }
    }

    #[derive(Debug)]
    pub struct PerfCounter {
        start: std::time::Instant,
        pub name: String,
        pub elapsed: u64,
        pub counter: u64,
        pub min: u64,
        pub max: u64,
        pub meta: u64,
    }

    impl PerfCounter {
        pub fn new(name: &str) -> Self {
            Self {
                name: name.to_owned(),
                ..Default::default()
            }
        }

        pub fn add(&mut self, other: PerfCounter) {
            self.name = other.name;
            self.elapsed += other.elapsed;
            self.counter += other.counter;
            self.min = self.min.min(other.min);
            self.max = self.max.max(other.max);
            self.meta += other.meta;
        }

        pub fn is_empty(&self) -> bool {
            self.counter == 0
        }

        pub fn measure(&mut self) -> PerfCounterGuard {
            PerfCounterGuard::new(self)
        }

        pub fn calc_meta(&mut self) {
            let duration = self.start.elapsed().as_micros() as u64;

            self.meta = duration - self.elapsed;
        }
    }

    impl Default for PerfCounter {
        fn default() -> Self {
            Self {
                start: std::time::Instant::now(),
                name: Default::default(),
                elapsed: Default::default(),
                counter: Default::default(),
                min: u64::MAX,
                max: Default::default(),
                meta: Default::default(),
            }
        }
    }

    impl std::ops::Add for PerfCounter {
        type Output = Self;

        fn add(self, rhs: Self) -> Self::Output {
            debug_assert_eq!(self.name, rhs.name);

            Self {
                name: self.name,
                start: self.start,
                elapsed: self.elapsed + rhs.elapsed,
                counter: self.counter + rhs.counter,
                min: self.min + rhs.min,
                max: self.max + rhs.max,
                meta: self.meta + rhs.meta,
            }
        }
    }

    impl std::fmt::Display for PerfCounter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_fmt(format_args!(
            "{: <30} avg: {: >5}μs, samples: {: >5}, min: {: >5}μs, max: {: >5}μs, meta: {: >5}μs",
            self.name,
            (self.elapsed as f64 / self.counter as f64) as u64,
            self.counter,
            self.min,
            self.max,
            self.meta / self.counter
        ))
        }
    }
}
