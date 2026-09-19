//! GENA subscription bookkeeping and outbound event delivery for a single
//! UPnP service (AVTransport or RenderingControl each get their own
//! [`EventManager`] instance — subscriptions are scoped per service, not
//! shared across them).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;
use tracing::warn;
use uuid::Uuid;

const DEFAULT_TIMEOUT_SECS: u64 = 1800;
const MIN_TIMEOUT_SECS: u64 = 30;
const MAX_TIMEOUT_SECS: u64 = 1800;

struct Subscriber {
    callback_urls: Vec<String>,
    next_seq: u32,
    expires_at: Instant,
}

/// What a caller needs right after accepting a new subscription: the SID and
/// TIMEOUT to put in the SUBSCRIBE response headers, and the callback URLs
/// to send the mandatory initial event to (SEQ 0).
pub struct NewSubscription {
    pub sid: String,
    pub timeout: Duration,
    pub callback_urls: Vec<String>,
}

#[derive(Debug, Error)]
pub enum SubscribeError {
    #[error("missing or empty CALLBACK header")]
    MissingCallback,
    #[error("unknown subscription id")]
    UnknownSid,
}

pub struct EventManager {
    subscribers: Mutex<HashMap<String, Subscriber>>,
    http: reqwest::Client,
}

impl EventManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            subscribers: Mutex::new(HashMap::new()),
            http: reqwest::Client::new(),
        })
    }

    /// A first-time SUBSCRIBE (has a CALLBACK header, no SID).
    pub fn subscribe(
        &self,
        callback_header: Option<&str>,
        timeout_header: Option<&str>,
    ) -> Result<NewSubscription, SubscribeError> {
        let callback_urls = parse_callback_urls(callback_header.unwrap_or(""));
        if callback_urls.is_empty() {
            return Err(SubscribeError::MissingCallback);
        }

        let sid = format!("uuid:{}", Uuid::new_v4());
        let timeout = parse_timeout(timeout_header);

        self.subscribers.lock().unwrap().insert(
            sid.clone(),
            Subscriber {
                callback_urls: callback_urls.clone(),
                // The initial event (SEQ 0) is the caller's job right after
                // this returns — the next broadcast from here starts at 1.
                next_seq: 1,
                expires_at: Instant::now() + timeout,
            },
        );

        Ok(NewSubscription {
            sid,
            timeout,
            callback_urls,
        })
    }

    /// A renewal SUBSCRIBE (has a SID header, no CALLBACK/NT).
    pub fn renew(
        &self,
        sid: &str,
        timeout_header: Option<&str>,
    ) -> Result<Duration, SubscribeError> {
        let mut subs = self.subscribers.lock().unwrap();
        let sub = subs.get_mut(sid).ok_or(SubscribeError::UnknownSid)?;
        let timeout = parse_timeout(timeout_header);
        sub.expires_at = Instant::now() + timeout;
        Ok(timeout)
    }

    pub fn unsubscribe(&self, sid: &str) -> Result<(), SubscribeError> {
        self.subscribers
            .lock()
            .unwrap()
            .remove(sid)
            .map(|_| ())
            .ok_or(SubscribeError::UnknownSid)
    }

    #[cfg(test)]
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().unwrap().len()
    }

    /// Sends the mandatory initial event (SEQ 0) to a subscription just
    /// returned by [`Self::subscribe`]. Fire-and-forget — doesn't block the
    /// SUBSCRIBE response on delivery.
    pub fn notify_initial(self: &Arc<Self>, sub: &NewSubscription, body: String) {
        let this = Arc::clone(self);
        let sid = sub.sid.clone();
        let urls = sub.callback_urls.clone();
        tokio::spawn(async move { this.deliver(&urls, &sid, 0, &body).await });
    }

    /// Sends `body` to every current subscriber, each with its own next SEQ,
    /// pruning any that have expired first. Fire-and-forget.
    pub fn broadcast(self: &Arc<Self>, body: String) {
        let targets = {
            let mut subs = self.subscribers.lock().unwrap();
            let now = Instant::now();
            subs.retain(|_, s| s.expires_at > now);
            subs.iter_mut()
                .map(|(sid, s)| {
                    let seq = s.next_seq;
                    s.next_seq = s.next_seq.wrapping_add(1);
                    (sid.clone(), s.callback_urls.clone(), seq)
                })
                .collect::<Vec<_>>()
        };

        for (sid, urls, seq) in targets {
            let this = Arc::clone(self);
            let body = body.clone();
            tokio::spawn(async move { this.deliver(&urls, &sid, seq, &body).await });
        }
    }

    /// Tries each callback URL in turn until one accepts the NOTIFY.
    async fn deliver(&self, callback_urls: &[String], sid: &str, seq: u32, body: &str) {
        for url in callback_urls {
            match self.send_one(url, sid, seq, body).await {
                Ok(()) => return,
                Err(e) => warn!(url, sid, seq, error = %e, "NOTIFY delivery failed"),
            }
        }
        warn!(sid, seq, "NOTIFY delivery failed for every callback URL");
    }

    async fn send_one(&self, url: &str, sid: &str, seq: u32, body: &str) -> Result<(), String> {
        let method = reqwest::Method::from_bytes(b"NOTIFY").map_err(|e| e.to_string())?;
        let response = self
            .http
            .request(method, url)
            .header("CONTENT-TYPE", "text/xml; charset=\"utf-8\"")
            .header("NT", "upnp:event")
            .header("NTS", "upnp:propchange")
            .header("SID", sid)
            .header("SEQ", seq.to_string())
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("subscriber responded with {}", response.status()))
        }
    }
}

/// Extracts every `<...>` URL from a CALLBACK header, e.g.
/// `<http://host:1/cb><http://host:2/cb>`. Silently drops anything that
/// isn't a well-formed `http(s)://` URL rather than rejecting the whole
/// subscription over one bad entry.
fn parse_callback_urls(header: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = header;

    while let Some(start) = rest.find('<') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('>') else {
            break;
        };
        let url = &after_start[..end];
        if url.starts_with("http://") || url.starts_with("https://") {
            urls.push(url.to_string());
        }
        rest = &after_start[end + 1..];
    }

    urls
}

/// Parses a `TIMEOUT: Second-1800` header, clamped to a sane range. Anything
/// unparsable (missing, malformed, or `Second-infinite`) falls back to the
/// default rather than granting an unbounded subscription.
fn parse_timeout(header: Option<&str>) -> Duration {
    let secs = header
        .and_then(|h| h.strip_prefix("Second-"))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);

    Duration::from_secs(secs.clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> Arc<EventManager> {
        EventManager::new()
    }

    #[test]
    fn subscribe_requires_a_callback_url() {
        let mgr = manager();
        assert!(matches!(
            mgr.subscribe(None, None),
            Err(SubscribeError::MissingCallback)
        ));
        assert!(matches!(
            mgr.subscribe(Some("not a url"), None),
            Err(SubscribeError::MissingCallback)
        ));
    }

    #[test]
    fn subscribe_parses_callback_urls_and_grants_a_sid() {
        let mgr = manager();
        let sub = mgr
            .subscribe(Some("<http://192.168.0.5:1234/cb>"), None)
            .unwrap();

        assert!(sub.sid.starts_with("uuid:"));
        assert_eq!(sub.callback_urls, vec!["http://192.168.0.5:1234/cb"]);
        assert_eq!(sub.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECS));
        assert_eq!(mgr.subscriber_count(), 1);
    }

    #[test]
    fn subscribe_parses_multiple_callback_urls() {
        let mgr = manager();
        let sub = mgr
            .subscribe(
                Some("<http://host:1/a><http://host:2/b>"),
                None,
            )
            .unwrap();

        assert_eq!(
            sub.callback_urls,
            vec!["http://host:1/a", "http://host:2/b"]
        );
    }

    #[test]
    fn timeout_header_is_honored_within_bounds() {
        let mgr = manager();
        let sub = mgr
            .subscribe(Some("<http://host/cb>"), Some("Second-60"))
            .unwrap();
        assert_eq!(sub.timeout, Duration::from_secs(60));
    }

    #[test]
    fn timeout_header_is_clamped_to_the_max() {
        let mgr = manager();
        let sub = mgr
            .subscribe(Some("<http://host/cb>"), Some("Second-999999"))
            .unwrap();
        assert_eq!(sub.timeout, Duration::from_secs(MAX_TIMEOUT_SECS));
    }

    #[test]
    fn infinite_timeout_falls_back_to_default() {
        let mgr = manager();
        let sub = mgr
            .subscribe(Some("<http://host/cb>"), Some("Second-infinite"))
            .unwrap();
        assert_eq!(sub.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECS));
    }

    #[test]
    fn renew_requires_a_known_sid() {
        let mgr = manager();
        assert!(matches!(
            mgr.renew("uuid:does-not-exist", None),
            Err(SubscribeError::UnknownSid)
        ));
    }

    #[test]
    fn renew_extends_an_existing_subscription() {
        let mgr = manager();
        let sub = mgr.subscribe(Some("<http://host/cb>"), None).unwrap();
        let timeout = mgr.renew(&sub.sid, Some("Second-120")).unwrap();
        assert_eq!(timeout, Duration::from_secs(120));
        assert_eq!(mgr.subscriber_count(), 1);
    }

    #[test]
    fn unsubscribe_removes_the_subscription() {
        let mgr = manager();
        let sub = mgr.subscribe(Some("<http://host/cb>"), None).unwrap();
        assert_eq!(mgr.subscriber_count(), 1);

        mgr.unsubscribe(&sub.sid).unwrap();
        assert_eq!(mgr.subscriber_count(), 0);
    }

    #[test]
    fn unsubscribe_rejects_an_unknown_sid() {
        let mgr = manager();
        assert!(matches!(
            mgr.unsubscribe("uuid:does-not-exist"),
            Err(SubscribeError::UnknownSid)
        ));
    }
}
