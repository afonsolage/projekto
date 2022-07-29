use once_cell::sync::Lazy;

use super::*;
use std::{collections::HashMap, sync::Mutex, time::Instant};

static PERF_MAP: Lazy<Mutex<PerfCounterMap>> = Lazy::new(Mutex::default);

pub(super) struct PerfCounterPlugin;

impl Plugin for PerfCounterPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(print_perf_counter);
    }
}

pub(super) fn print_perf_counter(input_keys: Res<Input<KeyCode>>, time: Res<Time>) {
    let ctrl = input_keys.any_pressed([KeyCode::LControl, KeyCode::RControl]);
    if input_keys.just_pressed(KeyCode::F12) {
        let mut guard = PERF_MAP.lock().unwrap();
        if ctrl {
            guard.0.clear();
            info!("Performance Counter cleared");
        } else {
            let mut output = String::default();
            let mut counters = guard.0.values().collect::<Vec<_>>();
            counters.sort_by(|&a, &b| b.elapsed.cmp(&a.elapsed));
            for p in counters {
                output += format!("{}\n", p).as_str();
            }

            info!("Time since startup: \n{}", time.seconds_since_startup());
            info!("Performance Counter: \n{}", output);
        }
    }
}

#[derive(Default)]
pub struct PerfCounterMap(HashMap<String, PerfCounter>);

impl PerfCounterMap {
    pub fn insert(&mut self, guard: &PerfCounterGuard) {
        let entry = self.0.entry(guard.name.clone()).or_insert(PerfCounter {
            name: guard.name.clone(),
            ..Default::default()
        });

        entry.elapsed += guard.elapsed;
        entry.counter += guard.counter;
        entry.min = entry.min.min(guard.min);
        entry.max = entry.max.max(guard.max);
        entry.meta += guard.meta;
    }
}

pub struct PerfMeasureGuard<'a>(&'a mut PerfCounterGuard, std::time::Instant);

impl<'a> PerfMeasureGuard<'a> {
    pub fn new(perf_counter: &'a mut PerfCounterGuard) -> Self {
        Self(perf_counter, std::time::Instant::now())
    }
}

impl<'a> Drop for PerfMeasureGuard<'a> {
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

#[derive(Debug, Clone)]
pub struct PerfCounter {
    pub name: String,
    pub elapsed: u64,
    pub counter: u64,
    pub min: u64,
    pub max: u64,
    pub meta: u64,
}

impl Default for PerfCounter {
    fn default() -> Self {
        Self {
            name: Default::default(),
            elapsed: Default::default(),
            counter: Default::default(),
            min: u64::MAX,
            max: Default::default(),
            meta: Default::default(),
        }
    }
}

impl std::fmt::Display for PerfCounter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{: <40} total: {: >20}μs, avg: {: >10}μs, samples: {: >5}, min: {: >5}μs, max: {: >5}μs, meta: {: >5}μs",
            self.name,
            self.elapsed,
            (self.elapsed as f64 / self.counter as f64) as u64,
            self.counter,
            self.min,
            self.max,
            self.meta / self.counter
        ))
    }
}

pub struct PerfCounterGuard {
    start: std::time::Instant,
    pub name: String,
    pub elapsed: u64,
    pub counter: u64,
    pub min: u64,
    pub max: u64,
    pub meta: u64,
}

impl PerfCounterGuard {
    pub fn measure(&mut self) -> PerfMeasureGuard {
        PerfMeasureGuard::new(self)
    }

    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    pub fn calc_meta(&mut self) {
        let duration = self.start.elapsed().as_micros() as u64;

        self.meta = duration - self.elapsed;
    }
}

impl Drop for PerfCounterGuard {
    fn drop(&mut self) {
        self.calc_meta();

        if self.counter > 0 {
            PERF_MAP.lock().unwrap().insert(self);
        }
    }
}

impl Default for PerfCounterGuard {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            name: Default::default(),
            elapsed: Default::default(),
            counter: Default::default(),
            min: u64::MAX,
            max: Default::default(),
            meta: Default::default(),
        }
    }
}
