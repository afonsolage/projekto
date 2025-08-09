use std::time::Instant;

use bevy::{platform::collections::HashMap, prelude::*};

#[derive(Default)]
pub struct MetricData {
    name: &'static str,
    runs: Vec<(Instant, Option<Instant>)>,
}

impl MetricData {
    pub fn new(name: &'static str) -> Self {
        Self { name, runs: vec![] }
    }

    pub fn begin(&mut self) {
        self.runs.push((Instant::now(), None));
    }

    pub fn end(&mut self) {
        let run = self.runs.last_mut().unwrap();
        run.1 = Some(Instant::now());
    }
}

#[derive(Resource, Default)]
pub struct Metrics(HashMap<&'static str, MetricData>);

impl Metrics {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn add(&mut self, mut data: MetricData) {
        if !data.runs.is_empty() {
            self.0
                .entry(data.name)
                .or_insert(MetricData::new(data.name))
                .runs
                .append(&mut data.runs);
        }
    }

    pub fn print(&self) {
        for (_name, data) in &self.0 {
            let runs = data.runs.len();

            if runs == 0 {
                continue;
            }

            let name = data.name;
            let durations = data
                .runs
                .iter()
                .filter(|r| r.1.is_some())
                .map(|(b, e)| (e.unwrap() - *b).as_millis())
                .collect::<Vec<_>>();

            let min = durations.iter().min().copied().unwrap_or_default();
            let max = durations.iter().max().copied().unwrap_or_default();
            let sum = durations.iter().sum::<u128>();
            let avg = sum / runs as u128;

            debug!(
                "[{name}] runs: {runs}, min: {min}ms, max: {max}ms, avg: {avg}ms, total: {sum}ms"
            );
        }
    }
}
