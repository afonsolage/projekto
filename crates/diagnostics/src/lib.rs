//! Runtime process diagnostics shared by the client and the server.

use std::time::Duration;

use bevy::prelude::*;

/// How often the process memory is sampled.
pub const MEMORY_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

const MIB: f64 = 1024.0 * 1024.0;
const GIB: f64 = 1024.0 * MIB;

/// Resident memory of the current process.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessMemory {
    /// Current resident set size, in bytes.
    pub rss_bytes: u64,
    /// Highest sampled resident set size since the process started, in bytes.
    pub peak_rss_bytes: u64,
}

impl ProcessMemory {
    pub fn record(&mut self, rss_bytes: u64) {
        self.rss_bytes = rss_bytes;
        self.peak_rss_bytes = self.peak_rss_bytes.max(rss_bytes);
    }

    fn sample(&mut self) {
        if let Some(stats) = memory_stats::memory_stats() {
            self.record(stats.physical_mem as u64);
        }
    }
}

/// Samples [`ProcessMemory`] once a second.
pub struct ProcessMemoryPlugin;

impl Plugin for ProcessMemoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProcessMemory>()
            .add_systems(Startup, sample_once)
            .add_systems(Update, sample_periodically);
    }
}

fn sample_once(mut memory: ResMut<ProcessMemory>) {
    memory.sample();
}

fn sample_periodically(
    time: Res<Time>,
    mut memory: ResMut<ProcessMemory>,
    mut timer: Local<Option<Timer>>,
) {
    let timer = timer.get_or_insert_with(|| {
        Timer::from_seconds(MEMORY_SAMPLE_INTERVAL.as_secs_f32(), TimerMode::Repeating)
    });

    if timer.tick(time.delta()).just_finished() {
        memory.sample();
    }
}

/// Formats a byte count as MiB below 1 GiB and as GiB above it.
pub fn format_bytes(bytes: u64) -> String {
    let bytes = bytes as f64;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else {
        format!("{:.1} MiB", bytes / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_keeps_the_peak() {
        let mut memory = ProcessMemory::default();

        memory.record(100);
        memory.record(300);
        memory.record(200);

        assert_eq!(memory.rss_bytes, 200);
        assert_eq!(memory.peak_rss_bytes, 300);
    }

    #[test]
    fn formats_mib_and_gib() {
        assert_eq!(format_bytes(0), "0.0 MiB");
        assert_eq!(format_bytes(512 * 1024 * 1024), "512.0 MiB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.00 GiB");
    }
}
