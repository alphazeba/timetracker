use chrono::{DateTime, Duration, Utc};

/// Formats a duration as a human-readable string.
/// - Less than 60 seconds: "42s"
/// - 60 seconds or more: components like "1h 23m 45s" (omit zero components except seconds)
pub fn format_elapsed(duration: Duration) -> String {
    let total_seconds = duration.num_seconds();

    if total_seconds < 60 {
        return format!("{}s", total_seconds);
    }

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{}h", hours));
        parts.push(format!("{}m", minutes)); // include minutes (even if 0) when hours present
    } else if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    // Always include seconds when there are hours or minutes
    parts.push(format!("{}s", seconds));

    parts.join(" ")
}

/// Returns the elapsed duration between a session's start time and a note's timestamp.
pub fn note_offset(session_start: DateTime<Utc>, note_time: DateTime<Utc>) -> Duration {
    note_time - session_start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_elapsed_zero() {
        assert_eq!(format_elapsed(Duration::seconds(0)), "0s");
    }

    #[test]
    fn test_format_elapsed_59s() {
        assert_eq!(format_elapsed(Duration::seconds(59)), "59s");
    }

    #[test]
    fn test_format_elapsed_60s() {
        assert_eq!(format_elapsed(Duration::seconds(60)), "1m 0s");
    }

    #[test]
    fn test_format_elapsed_3661s() {
        assert_eq!(format_elapsed(Duration::seconds(3661)), "1h 1m 1s");
    }

    #[test]
    fn test_format_elapsed_3600s() {
        assert_eq!(format_elapsed(Duration::seconds(3600)), "1h 0m 0s");
    }

    #[test]
    fn test_note_offset() {
        let start = DateTime::from_timestamp(1_000_000, 0).unwrap();
        let note_time = DateTime::from_timestamp(1_000_042, 0).unwrap();
        assert_eq!(note_offset(start, note_time), Duration::seconds(42));
    }
}
