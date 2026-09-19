pub fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// Parses a UPnP time target in `H+:MM:SS` or `H+:MM:SS.sss` form (as used by
/// `Seek`'s `Target` argument for the `REL_TIME`/`ABS_TIME` units, and by
/// clients like BubbleUPnP that carry the same fractional-seconds format they
/// use for DIDL-Lite `res@duration`) into a `Duration`.
pub fn parse_duration(value: &str) -> Option<std::time::Duration> {
    let mut parts = value.trim().splitn(3, ':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds_field = parts.next()?;
    let (whole_seconds, fraction) = match seconds_field.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (seconds_field, ""),
    };
    let seconds: u64 = whole_seconds.parse().ok()?;
    if minutes >= 60 || seconds >= 60 {
        return None;
    }
    // Sub-second precision isn't tracked; round to the nearest second so a
    // target like "0:00:20.600" doesn't get truncated a whole second short.
    let round_up = fraction.chars().next().is_some_and(|c| c >= '5');
    let total = hours * 3600 + minutes * 60 + seconds + u64::from(round_up);

    Some(std::time::Duration::from_secs(total))
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

    #[test]
    fn parses_fractional_seconds() {
        assert_eq!(parse_duration("0:00:20.000"), Some(Duration::from_secs(20)));
        assert_eq!(parse_duration("0:00:20.200"), Some(Duration::from_secs(20)));
        assert_eq!(parse_duration("0:00:20.500"), Some(Duration::from_secs(21)));
        assert_eq!(parse_duration("0:00:20.999"), Some(Duration::from_secs(21)));
        assert_eq!(
            parse_duration("0:00:51.000"),
            Some(Duration::from_secs(51))
        );
    }
}
