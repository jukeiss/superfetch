use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fs, thread};

use super::{Fields, Kind, Module, Request};
use crate::sys::{self, CpuTicks};
use crate::{config, format};

pub const MODULE: Module = Module {
    name: "cpu",
    kind: Kind::Info,
    label: "CPU",
    format: "{model}{? ({cores})}{? @ {freq}}{? · {usage}}",
    about: "Processor model, core count, clock speed and utilisation.",
    fields: &["model", "cores", "freq", "usage"],
    settings: "# How long to measure {usage} for, in milliseconds. At 0 nothing waits:\n\
               # the reading covers the time since superfetch last ran (see the README).\n\
               sample_ms = 0\n",
    threaded: true,
    fetch,
};

fn fetch(req: &Request) -> Fields {
    let mut fields = Vec::new();
    if req.wants("model") {
        fields.extend(sys::cpu_model().map(|model| ("model", model)));
    }
    if req.wants("cores") {
        fields.push(("cores", sys::cpu_cores().to_string()));
    }
    if req.wants("freq") {
        fields.extend(sys::cpu_mhz().map(|mhz| ("freq", format::frequency(mhz))));
    }
    if req.wants("usage") {
        let usage = match req.settings.sample_ms.unwrap_or(0) {
            0 => estimate(),
            window => measure(Duration::from_millis(window)),
        };
        fields.extend(usage.map(|percent| ("usage", req.theme.percent(percent))));
    }
    fields
}

/// Utilisation between two samples of the tick counters, in percent.
fn utilisation(earlier: CpuTicks, later: CpuTicks) -> Option<f64> {
    let total = later.total.checked_sub(earlier.total).filter(|t| *t > 0)?;
    let busy = later.busy.checked_sub(earlier.busy)?;
    Some((busy as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

/// The straightforward way: sample, wait, sample again.
fn measure(window: Duration) -> Option<f64> {
    let earlier = sys::cpu_ticks()?;
    thread::sleep(window);
    utilisation(earlier, sys::cpu_ticks()?)
}

/// A sample stored by one run for the next one to compare against.
#[derive(Debug, PartialEq)]
struct Snapshot {
    /// Milliseconds since the Unix epoch.
    taken: u64,
    ticks: CpuTicks,
    /// The reading that run arrived at.
    usage: Option<f64>,
}

/// Below this the counters have not moved enough to divide.
const MIN_AGE_MS: u64 = 250;
/// Above this the average no longer says much about the present.
const MAX_AGE_MS: u64 = 5 * 60 * 1000;

/// Utilisation without waiting.
///
/// The kernel only exposes counters, so a reading needs two samples some
/// time apart. Instead of sleeping between them, each run leaves its
/// sample in the cache directory and the next run measures against it.
/// When the stored sample is too old, the one-minute load average stands
/// in until there is a fresh pair.
fn estimate() -> Option<f64> {
    let ticks = sys::cpu_ticks()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let path = cache_file();
    let previous = path
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| Snapshot::parse(&text));

    let age = previous.as_ref().map(|p| now.saturating_sub(p.taken));
    let usage = match (&previous, age) {
        // Run twice in quick succession: reuse the last reading and leave
        // the stored sample alone so it can keep ageing.
        (Some(previous), Some(age)) if age < MIN_AGE_MS => return previous.usage.or_else(load),
        (Some(previous), Some(age)) if age <= MAX_AGE_MS => {
            utilisation(previous.ticks, ticks).or_else(load)
        }
        _ => load(),
    };

    if let Some(path) = path {
        let snapshot = Snapshot {
            taken: now,
            ticks,
            usage,
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, snapshot.serialize());
    }
    usage
}

/// The share of cores the one-minute load average keeps busy.
fn load() -> Option<f64> {
    let load = sys::load_average()?;
    Some((load / sys::cpu_cores() as f64 * 100.0).clamp(0.0, 100.0))
}

impl Snapshot {
    fn parse(text: &str) -> Option<Snapshot> {
        let mut parts = text.split_whitespace();
        Some(Snapshot {
            taken: parts.next()?.parse().ok()?,
            ticks: CpuTicks {
                busy: parts.next()?.parse().ok()?,
                total: parts.next()?.parse().ok()?,
            },
            usage: parts.next().and_then(|usage| usage.parse().ok()),
        })
    }

    fn serialize(&self) -> String {
        let usage = self
            .usage
            .map_or("-".to_string(), |usage| format!("{usage:.1}"));
        format!(
            "{} {} {} {usage}\n",
            self.taken, self.ticks.busy, self.ticks.total
        )
    }
}

/// `$XDG_CACHE_HOME/superfetch/cpu`, falling back to `~/.cache` on Unix
/// and `%LOCALAPPDATA%` on Windows.
fn cache_file() -> Option<PathBuf> {
    let base = env::var_os("XDG_CACHE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            if cfg!(windows) {
                env::var_os("LOCALAPPDATA").map(PathBuf::from)
            } else {
                config::home_dir().map(|home| home.join(".cache"))
            }
        })?;
    Some(base.join("superfetch").join("cpu"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utilisation_is_the_busy_share_of_the_interval() {
        let ticks = |busy, total| CpuTicks { busy, total };
        assert_eq!(utilisation(ticks(100, 1000), ticks(150, 1200)), Some(25.0));
        assert_eq!(utilisation(ticks(100, 1000), ticks(100, 1000)), None);
        // Counters that went backwards mean a reboot in between.
        assert_eq!(utilisation(ticks(900, 9000), ticks(10, 100)), None);
    }

    #[test]
    fn snapshots_round_trip() {
        let snapshot = Snapshot {
            taken: 1_760_000_000_000,
            ticks: CpuTicks {
                busy: 42,
                total: 4242,
            },
            usage: Some(12.5),
        };
        assert_eq!(Snapshot::parse(&snapshot.serialize()), Some(snapshot));

        let unknown = Snapshot::parse("5 1 2 -\n").unwrap();
        assert_eq!(unknown.usage, None);
        assert_eq!(Snapshot::parse("garbage"), None);
    }
}
