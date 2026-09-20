//! Parsing for inbound `M-SEARCH * HTTP/1.1` requests (SSDP, UPnP Device
//! Architecture Annex A). Kept separate from [`super::messages`], which only
//! builds *outbound* SSDP messages (NOTIFY/response/M-SEARCH) — this module
//! is the other direction: turning a raw datagram a control point sent us
//! into a typed, validated request.

use crate::discovery::error::DiscoveryServiceError;
use std::collections::HashMap;
use std::time::Duration;

/// A parsed, validated `M-SEARCH` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MSearchRequest {
    pub search_target: String,
    pub mx: Duration,
}

impl MSearchRequest {
    /// Used when `MX` is missing or unparseable. SSDP requires compliant
    /// control points to send 1-5, but devices are expected to tolerate
    /// malformed/missing values rather than reject the request outright.
    const DEFAULT_MX_SECS: u64 = 1;
    /// Values above this are clamped — a client asking for an unreasonably
    /// long randomized delay shouldn't make us sit on the response forever.
    const MAX_MX_SECS: u64 = 5;

    /// Parses and validates a raw M-SEARCH datagram. Requires the
    /// `M-SEARCH * HTTP/1.1` request line, a `MAN: "ssdp:discover"` header,
    /// and an `ST` (search target) header — the three things the SSDP spec
    /// makes mandatory for a device to act on the request at all.
    pub fn parse(raw: &str) -> Result<Self, DiscoveryServiceError> {
        let mut lines = raw.split("\r\n").flat_map(|line| line.split('\n'));

        let request_line = lines
            .next()
            .ok_or_else(|| DiscoveryServiceError::ProtocolError("empty SSDP message".into()))?;

        if !request_line
            .trim_end()
            .eq_ignore_ascii_case("M-SEARCH * HTTP/1.1")
        {
            return Err(DiscoveryServiceError::ProtocolError(format!(
                "not an M-SEARCH request line: {}",
                request_line
            )));
        }

        let headers: HashMap<String, String> = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_ascii_uppercase(), value.trim().to_string()))
            })
            .collect();

        let man = headers
            .get("MAN")
            .ok_or_else(|| DiscoveryServiceError::MissingField("MAN".to_string()))?;
        if man.trim_matches('"') != "ssdp:discover" {
            return Err(DiscoveryServiceError::InvalidValue(format!(
                "unexpected MAN header: {}",
                man
            )));
        }

        let search_target = headers
            .get("ST")
            .ok_or_else(|| DiscoveryServiceError::MissingField("ST".to_string()))?
            .clone();

        let mx = headers
            .get("MX")
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_MX_SECS)
            .min(Self::MAX_MX_SECS);

        Ok(Self {
            search_target,
            mx: Duration::from_secs(mx),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(extra_headers: &str) -> String {
        format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\n{}\r\n\r\n",
            extra_headers
        )
    }

    #[test]
    fn parses_valid_request() {
        let parsed = MSearchRequest::parse(&request("ST: ssdp:all\r\nMX: 3")).unwrap();
        assert_eq!(parsed.search_target, "ssdp:all");
        assert_eq!(parsed.mx, Duration::from_secs(3));
    }

    #[test]
    fn defaults_mx_when_missing() {
        let parsed = MSearchRequest::parse(&request("ST: ssdp:all")).unwrap();
        assert_eq!(parsed.mx, Duration::from_secs(1));
    }

    #[test]
    fn clamps_mx_above_max() {
        let parsed = MSearchRequest::parse(&request("ST: ssdp:all\r\nMX: 120")).unwrap();
        assert_eq!(parsed.mx, Duration::from_secs(5));
    }

    #[test]
    fn header_names_are_case_insensitive() {
        let raw = "M-SEARCH * HTTP/1.1\r\nman: \"ssdp:discover\"\r\nst: upnp:rootdevice\r\n\r\n";
        let parsed = MSearchRequest::parse(raw).unwrap();
        assert_eq!(parsed.search_target, "upnp:rootdevice");
    }

    #[test]
    fn rejects_non_msearch_request_line() {
        assert!(MSearchRequest::parse("NOTIFY * HTTP/1.1\r\n\r\n").is_err());
    }

    #[test]
    fn rejects_missing_man_header() {
        assert!(MSearchRequest::parse("M-SEARCH * HTTP/1.1\r\nST: ssdp:all\r\n\r\n").is_err());
    }

    #[test]
    fn rejects_wrong_man_header() {
        let raw = "M-SEARCH * HTTP/1.1\r\nMAN: \"foo\"\r\nST: ssdp:all\r\n\r\n";
        assert!(MSearchRequest::parse(raw).is_err());
    }

    #[test]
    fn rejects_missing_search_target() {
        assert!(MSearchRequest::parse(&request("")).is_err());
    }
}
