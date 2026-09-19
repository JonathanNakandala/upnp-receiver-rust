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

/// The rates this renderer accepts for `Play`'s `Speed` argument, matched
/// against the parsed value from [`parse_play_speed`] (float comparison is
/// safe here since these are all exact binary fractions). Reverse playback
/// isn't included — negative-rate seeking needs support from every element
/// in the pipeline (source, demuxer, decoder), which most audio decoders
/// don't have.
pub const SUPPORTED_PLAY_SPEEDS: &[f64] = &[0.25, 0.5, 1.0, 2.0, 4.0];

/// Parses a UPnP `TransportPlaySpeed` value — a plain number (`"1"`, `"2"`)
/// or a fraction (`"1/2"`) — into a rate multiplier. Doesn't check it
/// against [`SUPPORTED_PLAY_SPEEDS`]; callers do that separately so they can
/// tell "malformed" apart from "well-formed but unsupported".
pub fn parse_play_speed(value: &str) -> Option<f64> {
    let value = value.trim();
    if let Ok(speed) = value.parse::<f64>() {
        return Some(speed);
    }
    let (numerator, denominator) = value.split_once('/')?;
    let numerator: f64 = numerator.trim().parse().ok()?;
    let denominator: f64 = denominator.trim().parse().ok()?;
    if denominator == 0.0 {
        return None;
    }
    Some(numerator / denominator)
}

/// The inverse of [`parse_play_speed`] for the subset of rates this
/// renderer actually accepts — used to render `GetTransportInfo`'s
/// `CurrentSpeed`. UPnP control points expect the same fraction notation
/// back (e.g. `"1/2"`, not `"0.5"`).
pub fn format_play_speed(speed: f64) -> String {
    const KNOWN: &[(f64, &str)] = &[
        (0.25, "1/4"),
        (0.5, "1/2"),
        (1.0, "1"),
        (2.0, "2"),
        (4.0, "4"),
    ];
    for (value, label) in KNOWN {
        if (speed - value).abs() < f64::EPSILON {
            return (*label).to_string();
        }
    }
    // Shouldn't happen for a speed that went through SUPPORTED_PLAY_SPEEDS,
    // but a plain decimal is still a valid TransportPlaySpeed string.
    speed.to_string()
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

    #[test]
    fn parses_play_speed_values() {
        assert_eq!(parse_play_speed("1"), Some(1.0));
        assert_eq!(parse_play_speed("2"), Some(2.0));
        assert_eq!(parse_play_speed("1/2"), Some(0.5));
        assert_eq!(parse_play_speed("1/4"), Some(0.25));
        assert_eq!(parse_play_speed(" 1 / 2 "), Some(0.5));
    }

    #[test]
    fn rejects_malformed_play_speed() {
        assert_eq!(parse_play_speed(""), None);
        assert_eq!(parse_play_speed("fast"), None);
        assert_eq!(parse_play_speed("1/0"), None);
        assert_eq!(parse_play_speed("1/"), None);
    }

    #[test]
    fn play_speed_round_trips_through_supported_values() {
        for &speed in SUPPORTED_PLAY_SPEEDS {
            let formatted = format_play_speed(speed);
            assert_eq!(
                parse_play_speed(&formatted),
                Some(speed),
                "round-trip failed for {speed}"
            );
        }
    }
}
