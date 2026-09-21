//! Background read-ahead. After `read_next()` resolves a chunk, it triggers
//! [`Prefetcher::trigger`], which opportunistically fetches the next few
//! chunks into the shared cache before the read cursor gets there - sized
//! to recently measured network throughput rather than a fixed window, so a
//! fast link builds a deeper cushion and a slow one doesn't waste bandwidth
//! chasing chunks the reactive path in `read_next()` would fetch just as
//! well on demand.
//!
//! Backend-lifetime (held in [`super::imp::Shared`]), for the same reason
//! the chunk cache itself is: it survives a pipeline rebuild (an audio/video
//! swap, decoder-stall recovery) instead of resetting its throughput
//! estimate back to "unknown" every time.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::Url;
use tracing::debug;

use super::cache::{chunk_index, ChunkCache, TrackKey};
use super::imp::{fetch_chunk, RUNTIME};

/// Chunks to stay ahead by, at minimum - attempted regardless of measured
/// throughput, since even a slow link benefits from some cushion against
/// per-request latency jitter.
const MIN_WINDOW_CHUNKS: u64 = 2;
/// Upper bound on how far ahead to prefetch, even on a fast link - there's
/// no benefit to racing far ahead of playback, and it bounds how much
/// in-flight work a single seek can make moot.
const MAX_WINDOW_CHUNKS: u64 = 8;
/// Throughput at/above which the window is already at its max - a
/// deliberately rough "comfortably faster than even a high-bitrate 4K
/// stream needs" threshold (~40 Mbps), not a measurement of the actual
/// stream's bitrate, which isn't known at this layer.
const FAST_THROUGHPUT_BYTES_PER_SEC: f64 = 5.0 * 1024.0 * 1024.0;

struct Throughput {
    /// Exponential moving average of recent fetch throughput, in bytes/sec.
    /// 0.0 means "no sample yet" - stay at the conservative minimum window
    /// until at least one fetch (reactive or prefetched) has completed.
    avg_bytes_per_sec: f64,
}

impl Throughput {
    fn record(&mut self, bytes: usize, elapsed: Duration) {
        let secs = elapsed.as_secs_f64();
        if secs <= 0.0 || bytes == 0 {
            return;
        }
        let sample = bytes as f64 / secs;
        // Weights the newest sample fairly heavily - network conditions on
        // a home LAN/phone-hosted server can shift quickly (another device
        // joining the network, the server's own disk contending with
        // something else), and a read-ahead window that's slow to react to
        // a slowdown risks fetching chunks that won't pay off before a
        // stall, while one slow to react to a speedup just under-uses
        // otherwise-idle bandwidth - a smaller cost either way, so biasing
        // toward responsiveness.
        const ALPHA: f64 = 0.3;
        self.avg_bytes_per_sec = if self.avg_bytes_per_sec <= 0.0 {
            sample
        } else {
            ALPHA * sample + (1.0 - ALPHA) * self.avg_bytes_per_sec
        };
    }

    fn window(&self) -> u64 {
        if self.avg_bytes_per_sec <= 0.0 {
            return MIN_WINDOW_CHUNKS;
        }
        let frac = (self.avg_bytes_per_sec / FAST_THROUGHPUT_BYTES_PER_SEC).clamp(0.0, 1.0);
        MIN_WINDOW_CHUNKS + ((MAX_WINDOW_CHUNKS - MIN_WINDOW_CHUNKS) as f64 * frac).round() as u64
    }
}

/// The pieces `trigger()` needs to know where to fetch from and how to
/// recognize a superseded fetch, bundled into one value instead of five-plus
/// separate parameters - same rationale as `backend.rs`'s `BackendHandles`.
pub struct PrefetchSource<'a> {
    pub cache: &'a Arc<Mutex<ChunkCache>>,
    pub client: &'a reqwest::Client,
    pub track: &'a TrackKey,
    pub uri: &'a Url,
    pub generation: &'a Arc<AtomicU64>,
}

pub struct Prefetcher {
    inflight: Mutex<HashSet<(TrackKey, u64)>>,
    throughput: Mutex<Throughput>,
}

impl Default for Prefetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Prefetcher {
    pub fn new() -> Self {
        Self {
            inflight: Mutex::new(HashSet::new()),
            throughput: Mutex::new(Throughput { avg_bytes_per_sec: 0.0 }),
        }
    }

    /// Kicks off background fetches for up to [`Throughput::window`] chunks
    /// after `from_index`, skipping any already cached or already in
    /// flight. Each spawned fetch checks `source.generation` against the
    /// snapshot taken here before writing its result back, so a
    /// `do_seek()` that supersedes this read-ahead - bumping the real
    /// counter behind it - makes any still-in-flight fetch from before it a
    /// no-op instead of polluting the cache with data for a position
    /// nobody's at anymore.
    pub fn trigger(self: &Arc<Self>, source: PrefetchSource<'_>, from_index: u64, total_size: Option<u64>) {
        let PrefetchSource { cache, client, track, uri, generation: current_generation } = source;

        let generation = current_generation.load(Ordering::Relaxed);
        let window = self.throughput.lock().unwrap().window();
        let last_index = total_size.map(|size| chunk_index(size.saturating_sub(1)));

        for offset in 1..=window {
            let index = from_index + offset;
            if let Some(last) = last_index {
                if index > last {
                    break;
                }
            }

            if cache.lock().unwrap().get(track, index).is_some() {
                continue;
            }
            {
                let mut inflight = self.inflight.lock().unwrap();
                if !inflight.insert((track.clone(), index)) {
                    continue; // Already being fetched, reactively or by an earlier prefetch.
                }
            }

            let this = self.clone();
            let cache = cache.clone();
            let client = client.clone();
            let uri = uri.clone();
            let track = track.clone();
            let current_generation = current_generation.clone();

            RUNTIME.spawn(async move {
                let started = Instant::now();
                let result = fetch_chunk(&client, &uri, index).await;
                this.inflight.lock().unwrap().remove(&(track.clone(), index));

                if current_generation.load(Ordering::Relaxed) != generation {
                    debug!(index, "upnpcachehttpsrc: prefetch superseded by a seek, dropping result");
                    return;
                }

                match result {
                    Ok(fetched) => {
                        this.throughput.lock().unwrap().record(fetched.data.len(), started.elapsed());
                        cache.lock().unwrap().insert(track, index, fetched.data);
                        debug!(index, "upnpcachehttpsrc: prefetched chunk");
                    }
                    Err(err) => {
                        // Not fatal: the reactive path in `read_next()` will
                        // just fetch it again if the read cursor actually
                        // gets there.
                        debug!(index, %err, "upnpcachehttpsrc: prefetch fetch failed");
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_throughput_uses_the_minimum_window() {
        let t = Throughput { avg_bytes_per_sec: 0.0 };
        assert_eq!(t.window(), MIN_WINDOW_CHUNKS);
    }

    #[test]
    fn fast_throughput_uses_the_maximum_window() {
        let t = Throughput { avg_bytes_per_sec: FAST_THROUGHPUT_BYTES_PER_SEC * 2.0 };
        assert_eq!(t.window(), MAX_WINDOW_CHUNKS);
    }

    #[test]
    fn moderate_throughput_lands_between_the_bounds() {
        let t = Throughput { avg_bytes_per_sec: FAST_THROUGHPUT_BYTES_PER_SEC / 2.0 };
        let window = t.window();
        assert!(window > MIN_WINDOW_CHUNKS && window < MAX_WINDOW_CHUNKS, "got {window}");
    }

    #[test]
    fn recording_a_sample_moves_the_average_toward_it() {
        let mut t = Throughput { avg_bytes_per_sec: 0.0 };
        t.record(1_000_000, Duration::from_secs(1));
        assert!((t.avg_bytes_per_sec - 1_000_000.0).abs() < 1.0, "first sample should set the average outright");

        t.record(0, Duration::from_secs(1));
        assert!(
            (t.avg_bytes_per_sec - 1_000_000.0).abs() < 1.0,
            "a zero-byte sample (e.g. a failed fetch) should not move the average"
        );
    }
}
