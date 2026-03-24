use anyhow::{bail, Context, Result};

// Timing constants reserved for future daemon and TUI polling configuration
#[allow(dead_code)]
pub const RESTART_THRESHOLD_SECS: u64 = 3;
#[allow(dead_code)]
pub const DAEMON_POLL_INTERVAL_MS: u64 = 500;
#[allow(dead_code)]
pub const TUI_POLL_INTERVAL_MS: u64 = 250;
pub const SPEED_PRESETS: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
pub const DEFAULT_SPEED_INDEX: usize = 2;

/// Parse "2:30" → 150.0, "1:02:30" → 3750.0, "90" → 90.0
pub fn parse_timestamp(input: &str) -> Result<f64> {
    let parts: Vec<&str> = input.split(':').collect();

    let parse_part = |s: &str, name: &str| -> Result<f64> {
        s.parse::<f64>()
            .with_context(|| format!("Invalid {}: '{}' — expected a number", name, s))
    };

    let secs = match parts.as_slice() {
        [s] => {
            let v = parse_part(s, "seconds")?;
            if v < 0.0 {
                bail!("Position cannot be negative");
            }
            v
        }
        [m, s] => {
            let m = parse_part(m, "minutes")?;
            let s = parse_part(s, "seconds")?;
            if s >= 60.0 {
                bail!("Seconds must be 0-59, got {}", s);
            }
            m * 60.0 + s
        }
        [h, m, s] => {
            let h = parse_part(h, "hours")?;
            let m = parse_part(m, "minutes")?;
            let s = parse_part(s, "seconds")?;
            if m >= 60.0 {
                bail!("Minutes must be 0-59, got {}", m);
            }
            if s >= 60.0 {
                bail!("Seconds must be 0-59, got {}", s);
            }
            h * 3600.0 + m * 60.0 + s
        }
        _ => bail!("Invalid timestamp format. Use: 90, 2:30, or 1:02:30"),
    };

    Ok(secs)
}

/// Parse "30m" → 30, "1h" → 60, "1h30m" → 90, "45" → 45 (minutes)
pub fn parse_duration_str(input: &str) -> Result<u32> {
    let input = input.to_lowercase();
    let mut total_minutes = 0u32;
    let mut current_num = String::new();

    for c in input.chars() {
        match c {
            '0'..='9' => current_num.push(c),
            'h' => {
                let hours: u32 = current_num.parse().context("Invalid hours value")?;
                total_minutes += hours * 60;
                current_num.clear();
            }
            'm' => {
                let mins: u32 = current_num.parse().context("Invalid minutes value")?;
                total_minutes += mins;
                current_num.clear();
            }
            _ => bail!("Unknown duration unit. Use: 30m, 1h, 1h30m"),
        }
    }

    // Bare number = minutes
    if !current_num.is_empty() {
        total_minutes += current_num
            .parse::<u32>()
            .context("Invalid duration number")?;
    }

    if total_minutes == 0 {
        bail!("Duration must be > 0. Use: 30m, 1h, 1h30m");
    }

    Ok(total_minutes)
}

/// "2h 15m" format for stats display
pub fn format_duration_long(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    match (h, m) {
        (0, m) => format!("{}m", m),
        (h, 0) => format!("{}h", h),
        (h, m) => format!("{}h {}m", h, m),
    }
}

/// Get next speed preset up/down
pub fn next_speed_preset(current: f64, up: bool) -> f64 {
    let idx = SPEED_PRESETS
        .iter()
        .position(|&s| (s - current).abs() < 0.01)
        .unwrap_or(DEFAULT_SPEED_INDEX);
    let next = if up {
        (idx + 1).min(SPEED_PRESETS.len() - 1)
    } else {
        idx.saturating_sub(1)
    };
    SPEED_PRESETS[next]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_timestamp_seconds() {
        assert_eq!(parse_timestamp("90").unwrap(), 90.0);
    }
    #[test]
    fn parse_timestamp_min_sec() {
        assert_eq!(parse_timestamp("2:30").unwrap(), 150.0);
    }
    #[test]
    fn parse_timestamp_hr_min_sec() {
        assert_eq!(parse_timestamp("1:02:30").unwrap(), 3750.0);
    }
    #[test]
    fn parse_timestamp_invalid_seconds() {
        assert!(parse_timestamp("2:70").is_err());
    }
    #[test]
    fn parse_timestamp_garbage() {
        assert!(parse_timestamp("abc").is_err());
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration_str("30m").unwrap(), 30);
    }
    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration_str("1h").unwrap(), 60);
    }
    #[test]
    fn parse_duration_combined() {
        assert_eq!(parse_duration_str("1h30m").unwrap(), 90);
    }
    #[test]
    fn parse_duration_bare() {
        assert_eq!(parse_duration_str("45").unwrap(), 45);
    }
    #[test]
    fn parse_duration_zero() {
        assert!(parse_duration_str("0").is_err());
    }

    #[test]
    fn speed_preset_up() {
        assert_eq!(next_speed_preset(1.0, true), 1.25);
        assert_eq!(next_speed_preset(2.0, true), 2.0);
    }
    #[test]
    fn speed_preset_down() {
        assert_eq!(next_speed_preset(1.0, false), 0.75);
        assert_eq!(next_speed_preset(0.5, false), 0.5);
    }
}
