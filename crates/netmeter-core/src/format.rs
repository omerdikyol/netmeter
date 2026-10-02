use crate::config::UnitSystem;

const BINARY_UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
const DECIMAL_UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

fn scale(unit: UnitSystem) -> (f64, &'static [&'static str; 6]) {
    match unit {
        UnitSystem::Binary => (1024.0, &BINARY_UNITS),
        // ISPs and mobile carriers bill in decimal units, so "auto" follows that.
        UnitSystem::Auto | UnitSystem::Decimal => (1000.0, &DECIMAL_UNITS),
    }
}

/// Largest unit that keeps the value >= 1, with its index.
fn split(bytes: f64, base: f64, units: &'static [&'static str; 6]) -> (f64, usize) {
    let mut value = bytes;
    let mut idx = 0;
    while value >= base && idx < units.len() - 1 {
        value /= base;
        idx += 1;
    }
    (value, idx)
}

/// Human readable volume, e.g. `42.13 GB`.
pub fn format_bytes(bytes: u64, unit: UnitSystem) -> String {
    let (base, units) = scale(unit);
    let (value, idx) = split(bytes as f64, base, units);
    if idx == 0 {
        format!("{bytes} {}", units[0])
    } else {
        format!("{value:.2} {}", units[idx])
    }
}

/// Human readable rate, e.g. `1.24 MB/s`.
pub fn format_rate(bytes_per_sec: f64, unit: UnitSystem) -> String {
    let (base, units) = scale(unit);
    let (value, idx) = split(bytes_per_sec.max(0.0), base, units);
    if idx == 0 {
        format!("{value:.0} {}/s", units[0])
    } else {
        format!("{value:.2} {}/s", units[idx])
    }
}

/// Compact form for the menu bar title, e.g. `1.2M`, `340K`, `12B`.
pub fn format_compact(bytes_per_sec: f64) -> String {
    const UNITS: [(&str, f64); 5] = [
        ("B", 1.0),
        ("K", 1_000.0),
        ("M", 1_000_000.0),
        ("G", 1_000_000_000.0),
        ("T", 1_000_000_000_000.0),
    ];
    let v = bytes_per_sec.max(0.0);
    let mut chosen = UNITS[0];
    for pair in UNITS {
        if v >= pair.1 {
            chosen = pair;
        }
    }
    if chosen.1 == 1.0 {
        return format!("{:.0}B", v);
    }
    let scaled = v / chosen.1;
    if scaled < 10.0 {
        format!("{scaled:.1}{}", chosen.0)
    } else {
        format!("{scaled:.0}{}", chosen.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_decimal_units_by_default() {
        assert_eq!(format_bytes(1_500_000_000, UnitSystem::Auto), "1.50 GB");
        assert_eq!(format_bytes(999, UnitSystem::Auto), "999 B");
    }

    #[test]
    fn binary_units_when_requested() {
        assert_eq!(format_bytes(1024, UnitSystem::Binary), "1.00 KiB");
        assert_eq!(format_bytes(1024, UnitSystem::Decimal), "1.02 KB");
    }

    #[test]
    fn rates_are_readable() {
        assert_eq!(format_rate(1_240_000.0, UnitSystem::Auto), "1.24 MB/s");
        assert_eq!(format_rate(400.0, UnitSystem::Auto), "400 B/s");
    }

    #[test]
    fn compact_form_is_short() {
        assert_eq!(format_compact(0.0), "0B");
        assert_eq!(format_compact(1_200.0), "1.2K");
        assert_eq!(format_compact(1_200_000.0), "1.2M");
        assert_eq!(format_compact(34_000_000.0), "34M");
        assert_eq!(format_compact(1_000_000_000.0), "1.0G");
    }
}
