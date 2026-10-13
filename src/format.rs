//! Turning raw numbers into the short strings shown on screen.

use crate::config::UnitsConfig;

/// A byte count, scaled to the largest unit that keeps it at or above one.
pub fn size(bytes: u64, units: &UnitsConfig) -> String {
    let (base, names) = if units.binary {
        (1024.0, ["B", "KiB", "MiB", "GiB", "TiB", "PiB"])
    } else {
        (1000.0, ["B", "kB", "MB", "GB", "TB", "PB"])
    };
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= base && unit < names.len() - 1 {
        value /= base;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.*} {}", units.precision, names[unit])
    }
}

/// `2h 15m`, `7d 4m`, `38s`: the two or three largest non-zero units.
pub fn uptime(seconds: u64) -> String {
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3600 % 24, seconds / 60 % 60);
    if days == 0 && hours == 0 && minutes == 0 {
        return format!("{seconds}s");
    }
    [(days, 'd'), (hours, 'h'), (minutes, 'm')]
        .iter()
        .filter(|(value, _)| *value > 0)
        .map(|(value, unit)| format!("{value}{unit}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn frequency(mhz: u32) -> String {
    if mhz >= 1000 {
        format!("{:.2} GHz", f64::from(mhz) / 1000.0)
    } else {
        format!("{mhz} MHz")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        let binary = UnitsConfig::default();
        let decimal = UnitsConfig {
            binary: false,
            precision: 1,
        };
        assert_eq!(size(512, &binary), "512 B");
        assert_eq!(size(16 << 30, &binary), "16.00 GiB");
        assert_eq!(size(1536 << 20, &binary), "1.50 GiB");
        assert_eq!(size(500_107_862_016, &decimal), "500.1 GB");
    }

    #[test]
    fn uptimes() {
        assert_eq!(uptime(38), "38s");
        assert_eq!(uptime(8100), "2h 15m");
        assert_eq!(uptime(7 * 86_400 + 4 * 60), "7d 4m");
        assert_eq!(uptime(86_400 + 3600 + 60 + 1), "1d 1h 1m");
    }

    #[test]
    fn frequencies() {
        assert_eq!(frequency(4464), "4.46 GHz");
        assert_eq!(frequency(800), "800 MHz");
    }
}
