//! A bounded, in-memory, chunked cache of remote media bytes shared by every
//! [`super::imp::CacheHttpSrc`] instance for this backend's lifetime -
//! outliving any individual pipeline rebuild (audio/video swap, decoder-stall
//! recovery), which is the whole point: those rebuilds happen mid-playback,
//! exactly when losing prefetched data would hurt most.
//!
//! Deliberately simple: fixed-size chunks, plain LRU eviction by chunk count
//! (not exact byte accounting - `CHUNK_SIZE` is uniform for every chunk
//! except a track's last one, so bounding by count closely bounds bytes too
//! without the bookkeeping of a byte-exact cache). No cross-chunk
//! reassembly, no partial-chunk storage - a cache entry is either the whole
//! chunk or absent.

use std::num::NonZeroUsize;

use bytes::Bytes;
use lru::LruCache;

/// Size of one cached unit. Large enough that a cache hit serves a
/// meaningful amount of playback (fewer round trips through the cache/lock
/// per second of video), small enough that a cache miss's first-byte latency
/// stays low and the LRU has reasonably fine-grained eviction. 2 MiB covers
/// a couple of seconds of even a high-bitrate 4K HEVC stream.
pub const CHUNK_SIZE: u64 = 2 * 1024 * 1024;

/// Identifies which track a cached chunk belongs to, so a freshly loaded
/// track never serves another track's bytes at the same offset. The literal
/// URI is sufficient - this cache never needs to look tracks up by anything
/// else.
pub type TrackKey = String;

type ChunkKey = (TrackKey, u64);

/// Index of the chunk containing byte offset `position`.
pub fn chunk_index(position: u64) -> u64 {
    position / CHUNK_SIZE
}

/// Byte offset the given chunk index starts at.
pub fn chunk_start(chunk_index: u64) -> u64 {
    chunk_index * CHUNK_SIZE
}

pub struct ChunkCache {
    chunks: LruCache<ChunkKey, Bytes>,
}

impl ChunkCache {
    /// `budget_bytes` is approximate - capacity is tracked in chunk count
    /// (`budget_bytes / CHUNK_SIZE`), not exact bytes, since real chunks are
    /// uniform size except a track's final, shorter one. Always holds at
    /// least one chunk, even for a very small budget.
    pub fn new(budget_bytes: u64) -> Self {
        let capacity = (budget_bytes / CHUNK_SIZE).max(1) as usize;
        Self {
            chunks: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
        }
    }

    pub fn get(&mut self, track: &TrackKey, index: u64) -> Option<Bytes> {
        self.chunks.get(&(track.clone(), index)).cloned()
    }

    pub fn insert(&mut self, track: TrackKey, index: u64, data: Bytes) {
        self.chunks.put((track, index), data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str) -> TrackKey {
        name.to_string()
    }

    #[test]
    fn chunk_index_and_start_round_trip_within_a_chunk() {
        assert_eq!(chunk_index(0), 0);
        assert_eq!(chunk_index(CHUNK_SIZE - 1), 0);
        assert_eq!(chunk_index(CHUNK_SIZE), 1);
        assert_eq!(chunk_index(CHUNK_SIZE * 3 + 5), 3);
        assert_eq!(chunk_start(0), 0);
        assert_eq!(chunk_start(3), CHUNK_SIZE * 3);
    }

    #[test]
    fn miss_on_empty_cache() {
        let mut cache = ChunkCache::new(CHUNK_SIZE * 4);
        assert!(cache.get(&track("a"), 0).is_none());
    }

    #[test]
    fn hit_after_insert() {
        let mut cache = ChunkCache::new(CHUNK_SIZE * 4);
        cache.insert(track("a"), 0, Bytes::from_static(b"hello"));
        assert_eq!(cache.get(&track("a"), 0), Some(Bytes::from_static(b"hello")));
    }

    #[test]
    fn different_tracks_at_the_same_chunk_index_dont_collide() {
        let mut cache = ChunkCache::new(CHUNK_SIZE * 4);
        cache.insert(track("a"), 0, Bytes::from_static(b"from-a"));
        cache.insert(track("b"), 0, Bytes::from_static(b"from-b"));
        assert_eq!(cache.get(&track("a"), 0), Some(Bytes::from_static(b"from-a")));
        assert_eq!(cache.get(&track("b"), 0), Some(Bytes::from_static(b"from-b")));
    }

    #[test]
    fn evicts_least_recently_used_chunk_once_over_budget() {
        // Budget for exactly 2 chunks.
        let mut cache = ChunkCache::new(CHUNK_SIZE * 2);
        cache.insert(track("a"), 0, Bytes::from_static(b"c0"));
        cache.insert(track("a"), 1, Bytes::from_static(b"c1"));
        // Touch chunk 0 so chunk 1 becomes the least-recently-used one.
        assert!(cache.get(&track("a"), 0).is_some());
        cache.insert(track("a"), 2, Bytes::from_static(b"c2"));

        assert!(cache.get(&track("a"), 0).is_some(), "recently touched chunk should survive");
        assert!(cache.get(&track("a"), 1).is_none(), "least-recently-used chunk should be evicted");
        assert!(cache.get(&track("a"), 2).is_some(), "just-inserted chunk should be present");
    }

    #[test]
    fn budget_smaller_than_one_chunk_still_holds_one() {
        let mut cache = ChunkCache::new(1024);
        cache.insert(track("a"), 0, Bytes::from_static(b"x"));
        assert!(cache.get(&track("a"), 0).is_some());
    }
}
