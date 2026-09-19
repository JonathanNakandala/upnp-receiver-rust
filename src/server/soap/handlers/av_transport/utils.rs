pub fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// Parses a UPnP time target in `H+:MM:SS` form (as used by `Seek`'s `Target`
/// argument for the `REL_TIME`/`ABS_TIME` units) into a `Duration`.
pub fn parse_duration(value: &str) -> Option<std::time::Duration> {
    let mut parts = value.trim().splitn(3, ':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: u64 = parts.next()?.parse().ok()?;
    if minutes >= 60 || seconds >= 60 {
        return None;
    }
    Some(std::time::Duration::from_secs(
        hours * 3600 + minutes * 60 + seconds,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parses_valid_targets() {
        assert_eq!(parse_duration("00:00:00"), Some(Duration::from_secs(0)));
        assert_eq!(parse_duration("00:01:30"), Some(Duration::from_secs(90)));
        assert_eq!(
            parse_duration("01:02:03"),
            Some(Duration::from_secs(3723))
        );
    }

    #[test]
    fn rejects_malformed_targets() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("not-a-time"), None);
        assert_eq!(parse_duration("00:60:00"), None);
        assert_eq!(parse_duration("00:00:60"), None);
    }
}
