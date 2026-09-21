//! `upnpcachehttpsrc`: a `PushSrc`/`GstURIHandler` for `http`/`https`, backed
//! by [`super::cache::ChunkCache`] instead of `queue2`'s buffering modes.
//!
//! Structured the same way `gst-plugins-rs`'s own `reqwesthttpsrc` is (a
//! plain Rust `PushSrc` subclass, not a wrapper around `appsrc`) so it
//! autoplugs into `playbin`/`playbin3` exactly like `souphttpsrc` does,
//! with no changes needed anywhere else in this backend's pipeline code.
//!
//! The key difference from a normal HTTP source: `do_seek` never touches
//! the network. It only moves a cursor - the actual fetch-or-serve-from-
//! cache decision happens lazily, in `create()`, the next time data is
//! actually pulled. That split is what makes seeking (both directions)
//! return instantly regardless of whether the target ends up being a cache
//! hit or a miss.

use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use futures::{future, StreamExt};
use gstreamer as gst;
use gstreamer_base as gst_base;
use reqwest::{StatusCode, Url};
use tokio::runtime;

use tracing::{info, warn};

use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base::prelude::*;
use gst_base::subclass::base_src::CreateSuccess;
use gst_base::subclass::prelude::*;

use super::cache::{chunk_index, chunk_start, ChunkCache, TrackKey, CHUNK_SIZE};

/// Upper bound on a single buffer this element pushes downstream. Chunks
/// are fetched/cached whole (`CHUNK_SIZE`), but handed to the pipeline in
/// smaller pieces across several `create()` calls - GStreamer elements
/// downstream expect buffer sizes in this ballpark, not multi-megabyte
/// ones, and this keeps scheduling granularity normal.
const MAX_BUFFER_SIZE: usize = 128 * 1024;

/// Handles shared across every element instance and every pipeline rebuild
/// for this backend's lifetime - see the module docs on `cache_src` for why
/// this can't live inside the element itself. Injected via
/// [`CacheHttpSrc::set_shared`], normally called from `playbin`'s
/// `source-setup` signal.
#[derive(Clone)]
pub struct Shared {
    pub cache: Arc<Mutex<ChunkCache>>,
    pub client: reqwest::Client,
}

impl Shared {
    /// Used only if an instance is started without ever being injected
    /// (standalone `gst-launch-1.0` debugging, or a unit test) - a real
    /// pipeline built by this backend always injects a shared handle before
    /// playback starts pulling data.
    fn standalone_default() -> Self {
        const STANDALONE_CACHE_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
        Self {
            cache: Arc::new(Mutex::new(ChunkCache::new(STANDALONE_CACHE_BUDGET_BYTES))),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Default)]
struct Settings {
    location: Option<Url>,
}

#[derive(Default)]
enum State {
    #[default]
    Stopped,
    Started {
        uri: Url,
        position: u64,
        size: Option<u64>,
        seekable: bool,
        /// The chunk `create()` is currently draining, so consecutive calls
        /// within the same chunk touch neither the cache lock nor the
        /// network.
        current: Option<(u64, Bytes)>,
    },
}

#[derive(Default)]
enum Canceller {
    #[default]
    None,
    Handle(future::AbortHandle),
    Cancelled,
}

impl Canceller {
    fn abort(&mut self) {
        if let Canceller::Handle(ref handle) = *self {
            handle.abort();
        }
        *self = Canceller::Cancelled;
    }
}

#[derive(Default)]
pub struct CacheHttpSrc {
    settings: Mutex<Settings>,
    state: Mutex<State>,
    shared: Mutex<Option<Shared>>,
    canceller: Mutex<Canceller>,
}

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "upnpcachehttpsrc",
        gst::DebugColorFlags::empty(),
        Some("Cached HTTP source backed by a bounded in-memory chunk cache"),
    )
});

/// A couple of worker threads is plenty - every fetch here is one bounded
/// (`CHUNK_SIZE`) ranged GET, not a long-lived streamed response, so there's
/// never more than a handful of these in flight at once.
static RUNTIME: LazyLock<runtime::Runtime> = LazyLock::new(|| {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .unwrap()
});

struct FetchedChunk {
    data: Bytes,
    /// Total content length, if the server told us (`Content-Range`'s
    /// `/total` on a 206, or `Content-Length` on a 200 that ignored our
    /// range entirely).
    total_size: Option<u64>,
    /// A 206 response to our ranged request, or an explicit
    /// `Accept-Ranges: bytes` on a 200 - a server that neither honors nor
    /// advertises ranges gets treated as non-seekable, same as any other
    /// HTTP source would.
    seekable: bool,
}

async fn fetch_chunk(
    client: &reqwest::Client,
    uri: &Url,
    index: u64,
) -> Result<FetchedChunk, gst::ErrorMessage> {
    let start = chunk_start(index);
    let end = start + CHUNK_SIZE - 1;

    info!(index, start, end, %uri, "upnpcachehttpsrc: fetching chunk");

    let res = client
        .get(uri.clone())
        .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
        .send()
        .await
        .map_err(|err| {
            warn!(index, %uri, error = ?err, "upnpcachehttpsrc: request failed");
            gst::error_msg!(
                gst::ResourceError::OpenRead,
                ["Failed to fetch {}: {:?}", uri, err]
            )
        })?;

    if !res.status().is_success() {
        let status = res.status();
        warn!(index, %uri, %status, "upnpcachehttpsrc: non-success response");
        return Err(match status {
            StatusCode::NOT_FOUND => {
                gst::error_msg!(gst::ResourceError::NotFound, ["Resource '{}' not found", uri])
            }
            status => {
                gst::error_msg!(gst::ResourceError::OpenRead, ["Request for '{}' failed: {}", uri, status])
            }
        });
    }

    let partial = res.status() == StatusCode::PARTIAL_CONTENT;
    let headers = res.headers();
    info!(
        index,
        status = %res.status(),
        partial,
        content_length = ?headers.get(reqwest::header::CONTENT_LENGTH),
        content_range = ?headers.get(reqwest::header::CONTENT_RANGE),
        accept_ranges = ?headers.get(reqwest::header::ACCEPT_RANGES),
        "upnpcachehttpsrc: response headers"
    );

    let total_size = if partial {
        // `Content-Range: bytes <start>-<end>/<total>` - only the part
        // after '/' is needed, and "*" (unknown total) parses to None.
        headers
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|total| total.parse::<u64>().ok())
    } else {
        // Server ignored the Range header and sent the whole thing (200) -
        // its Content-Length is the total size, and this response's body
        // is the *entire* resource, not just our requested chunk.
        headers
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
    };

    let accept_ranges = headers
        .get(reqwest::header::ACCEPT_RANGES)
        .map(|v| v == "bytes")
        .unwrap_or(false);
    let seekable = partial || (accept_ranges && total_size.is_some());

    // A server that ignores our Range header sends the *entire* resource
    // starting at byte 0, not just this chunk - correct for chunk 0 (which
    // starts at 0 anyway), but any later chunk would need everything up to
    // its own end re-read from the start every time, unboundedly. That's
    // not a smaller/degraded chunk fetch, it's a fundamentally different
    // (sequential-only) access pattern this element doesn't implement - so
    // this fails clearly here rather than silently buffering gigabytes
    // trying to read past a chunk this response can't actually contain the
    // right bytes for.
    if !partial && index != 0 {
        warn!(index, %uri, "upnpcachehttpsrc: server ignored Range header for a non-zero chunk - can't serve it correctly");
        return Err(gst::error_msg!(
            gst::ResourceError::Seek,
            [
                "Server for '{}' ignores byte-range requests - can't seek to chunk {}",
                uri,
                index
            ]
        ));
    }

    // Bounded regardless of whether the server honored the range: a
    // non-partial response's `Content-Length` is the *whole resource*, and
    // reading it in full here (as `res.bytes()` would) means every
    // cache-miss fetch downloads the entire file before this call can even
    // return - which on a multi-gigabyte source looks indistinguishable
    // from a stuck pipeline (no frames, no error, just silence) until it
    // eventually finishes or this element's own timeout fires.
    let want = CHUNK_SIZE as usize;
    let mut data = BytesMut::with_capacity(want.min(1024 * 1024));
    let mut stream = res.bytes_stream();
    while data.len() < want {
        match stream.next().await {
            Some(Ok(bytes)) => data.extend_from_slice(&bytes),
            Some(Err(err)) => {
                warn!(index, %uri, error = ?err, "upnpcachehttpsrc: reading response body failed");
                return Err(gst::error_msg!(
                    gst::ResourceError::Read,
                    ["Failed to read response body for {}: {:?}", uri, err]
                ));
            }
            None => break, // End of body - fine if it's the file's last chunk.
        }
    }
    let got = data.len();
    data.truncate(want);
    info!(index, requested = want, got, "upnpcachehttpsrc: chunk fetched");

    Ok(FetchedChunk {
        data: data.freeze(),
        total_size,
        seekable,
    })
}

impl CacheHttpSrc {
    fn shared(&self) -> Shared {
        let mut shared = self.shared.lock().unwrap();
        if shared.is_none() {
            gst::warning!(
                CAT,
                imp = self,
                "Started without an injected cache/client - using a private, standalone one"
            );
            *shared = Some(Shared::standalone_default());
        }
        shared.clone().unwrap()
    }

    /// Called from `playbin`'s `source-setup` signal (see `backend.rs`) to
    /// hand this instance the backend-lifetime shared cache/client, before
    /// playback starts pulling data.
    pub fn set_shared(&self, shared: Shared) {
        *self.shared.lock().unwrap() = Some(shared);
    }

    fn set_location(&self, uri: Option<&str>) -> Result<(), glib::Error> {
        if let State::Started { .. } = *self.state.lock().unwrap() {
            return Err(glib::Error::new(
                gst::URIError::BadState,
                "Changing `location` on a started upnpcachehttpsrc is not supported",
            ));
        }

        let mut settings = self.settings.lock().unwrap();
        let Some(uri) = uri else {
            settings.location = None;
            return Ok(());
        };

        let parsed = Url::parse(uri).map_err(|err| {
            glib::Error::new(gst::URIError::BadUri, &format!("Failed to parse URI '{uri}': {err:?}"))
        })?;

        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(glib::Error::new(
                gst::URIError::UnsupportedProtocol,
                &format!("Unsupported URI scheme '{}'", parsed.scheme()),
            ));
        }

        settings.location = Some(parsed);
        Ok(())
    }

    /// Blocks the calling (streaming) thread until `future` resolves,
    /// timing out after a fixed bound and honoring `unlock()` from another
    /// thread the same way `reqwesthttpsrc` does - a flushing seek/state
    /// change must be able to interrupt a fetch in progress rather than
    /// wait it out.
    fn wait<F, T>(&self, future: F) -> Result<T, Option<gst::ErrorMessage>>
    where
        F: Send + Future<Output = Result<T, gst::ErrorMessage>>,
        T: Send + 'static,
    {
        const TIMEOUT: Duration = Duration::from_secs(15);

        let mut canceller = self.canceller.lock().unwrap();
        if matches!(*canceller, Canceller::Cancelled) {
            return Err(None);
        }
        let (abort_handle, abort_registration) = future::AbortHandle::new_pair();
        *canceller = Canceller::Handle(abort_handle);
        drop(canceller);

        let timed = async {
            match tokio::time::timeout(TIMEOUT, future).await {
                Ok(res) => res,
                Err(_) => Err(gst::error_msg!(gst::ResourceError::Read, ["Request timeout"])),
            }
        };
        let abortable = async {
            match future::Abortable::new(timed, abort_registration).await {
                Ok(res) => res.map_err(Some),
                Err(_) => Err(None),
            }
        };

        let res = RUNTIME.block_on(abortable);

        let mut canceller = self.canceller.lock().unwrap();
        if matches!(*canceller, Canceller::Cancelled) {
            return Err(None);
        }
        *canceller = Canceller::None;

        res
    }

    /// Ensures `current` (in `State::Started`) holds the chunk covering
    /// `position`, fetching-or-caching it if not, then returns up to
    /// `MAX_BUFFER_SIZE` bytes from `position` onward.
    fn read_next(&self) -> Result<Option<Bytes>, Option<gst::ErrorMessage>> {
        let (uri, position, size, current_idx) = {
            let state = self.state.lock().unwrap();
            match &*state {
                State::Started {
                    uri,
                    position,
                    size,
                    current,
                    ..
                } => (
                    uri.clone(),
                    *position,
                    *size,
                    current.as_ref().map(|(idx, _)| *idx),
                ),
                State::Stopped => {
                    warn!("upnpcachehttpsrc: read_next() called while Stopped");
                    return Err(None);
                }
            }
        };

        if let Some(size) = size {
            if position >= size {
                info!(position, size, "upnpcachehttpsrc: read_next() reached the end (position >= size)");
                return Ok(None);
            }
        }

        let index = chunk_index(position);
        let track: TrackKey = uri.to_string();
        let shared = self.shared();

        // Hoisted into its own statement rather than an `if let Some(cached)
        // = shared.cache.lock().unwrap().get(...) { .. } else { .. }` -
        // Rust extends a `MutexGuard` temporary created in an `if let`
        // condition to the end of the *whole* if/else chain, including the
        // `else` branch, regardless of whether the value it produced still
        // borrows from it. That guard would still be held (self-deadlocking
        // this exact thread) by the time the `else` branch below tries to
        // lock the same cache again to insert.
        let cache_hit = shared.cache.lock().unwrap().get(&track, index);

        let chunk = if current_idx == Some(index) {
            // Already holding this chunk from the previous call - common
            // case during normal linear playback, no lock/network touched.
            match &*self.state.lock().unwrap() {
                State::Started { current: Some((_, data)), .. } => data.clone(),
                _ => {
                    warn!(index, "upnpcachehttpsrc: expected `current` chunk to still be set but it wasn't (state changed concurrently)");
                    return Err(None);
                }
            }
        } else if let Some(cached) = cache_hit {
            cached
        } else {
            let fetched = self.wait(fetch_chunk(&shared.client, &uri, index))?;
            shared.cache.lock().unwrap().insert(track, index, fetched.data.clone());

            // The first fetch is also how total size/seekability get
            // discovered - fold that back into State once we have it.
            if size.is_none() {
                if let State::Started { size: size_slot, seekable, .. } = &mut *self.state.lock().unwrap() {
                    if size_slot.is_none() {
                        *size_slot = fetched.total_size;
                    }
                    *seekable = fetched.seekable;
                }
            }

            fetched.data
        };

        let start = chunk_start(index);
        let offset_in_chunk = (position - start).min(chunk.len() as u64) as usize;
        let take = (chunk.len() - offset_in_chunk).min(MAX_BUFFER_SIZE);
        if take == 0 {
            // Ran off the end of a short final/only chunk with no known
            // total size (a server that neither paginates nor reports
            // Content-Length) - treat it as end of stream.
            return Ok(None);
        }
        let slice = chunk.slice(offset_in_chunk..offset_in_chunk + take);

        if let State::Started { current, .. } = &mut *self.state.lock().unwrap() {
            *current = Some((index, chunk));
        }

        Ok(Some(slice))
    }
}

impl ObjectImpl for CacheHttpSrc {
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
            vec![glib::ParamSpecString::builder("location")
                .nick("Location")
                .blurb("URL to read from")
                .readwrite()
                .mutable_ready()
                .build()]
        });
        PROPERTIES.as_ref()
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        if pspec.name() == "location" {
            let location = value.get::<Option<&str>>().expect("type checked upstream");
            if let Err(err) = self.set_location(location) {
                gst::error!(CAT, imp = self, "Failed to set `location`: {:?}", err);
            }
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "location" => {
                let settings = self.settings.lock().unwrap();
                settings.location.as_ref().map(Url::to_string).to_value()
            }
            _ => unimplemented!(),
        }
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_automatic_eos(false);
        obj.set_format(gst::Format::Bytes);
    }
}

impl GstObjectImpl for CacheHttpSrc {}

impl ElementImpl for CacheHttpSrc {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
            gst::subclass::ElementMetadata::new(
                "Cached HTTP Source",
                "Source/Network/HTTP",
                "Read stream from an HTTP/HTTPS location through a bounded in-memory chunk cache",
                "upnp-receiver-rust",
            )
        });
        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            let caps = gst::Caps::new_any();
            vec![gst::PadTemplate::new("src", gst::PadDirection::Src, gst::PadPresence::Always, &caps).unwrap()]
        });
        PAD_TEMPLATES.as_ref()
    }
}

impl BaseSrcImpl for CacheHttpSrc {
    fn is_seekable(&self) -> bool {
        matches!(*self.state.lock().unwrap(), State::Started { seekable: true, .. })
    }

    fn size(&self) -> Option<u64> {
        match *self.state.lock().unwrap() {
            State::Started { size, .. } => size,
            State::Stopped => None,
        }
    }

    fn unlock(&self) -> Result<(), gst::ErrorMessage> {
        info!("upnpcachehttpsrc: unlock() - interrupting any in-flight fetch");
        self.canceller.lock().unwrap().abort();
        Ok(())
    }

    fn unlock_stop(&self) -> Result<(), gst::ErrorMessage> {
        info!("upnpcachehttpsrc: unlock_stop() - clearing cancellation");
        *self.canceller.lock().unwrap() = Canceller::None;
        Ok(())
    }

    fn start(&self) -> Result<(), gst::ErrorMessage> {
        let uri = self
            .settings
            .lock()
            .unwrap()
            .location
            .clone()
            .ok_or_else(|| gst::error_msg!(gst::CoreError::StateChange, ["Can't start without a location"]))?;

        info!(%uri, "upnpcachehttpsrc: start()");

        *self.state.lock().unwrap() = State::Started {
            uri: uri.clone(),
            position: 0,
            size: None,
            seekable: false,
            current: None,
        };

        // Probe chunk 0 synchronously, before returning, rather than
        // leaving `size`/`seekable` at their placeholder defaults until
        // the first `create()` call fetches it asynchronously. GStreamer
        // (matroskademux in particular) queries seekability once, early,
        // during pipeline negotiation - if that query lands before the
        // first fetch would otherwise complete, it sees the placeholder
        // `seekable: false` and bakes in "not seekable" for the rest of
        // this pipeline's life, so every later seek fails outright even
        // once the real answer is known. Blocking here costs one round
        // trip at track load (same as `reqwesthttpsrc`/`souphttpsrc` already
        // do), not per seek.
        let shared = self.shared();
        let fetched = self
            .wait(fetch_chunk(&shared.client, &uri, 0))
            .map_err(|err| err.unwrap_or_else(|| gst::error_msg!(gst::LibraryError::Failed, ["Interrupted during start"])))?;

        shared.cache.lock().unwrap().insert(uri.to_string(), 0, fetched.data.clone());
        if let State::Started { size, seekable, current, .. } = &mut *self.state.lock().unwrap() {
            *size = fetched.total_size;
            *seekable = fetched.seekable;
            *current = Some((0, fetched.data));
        }

        Ok(())
    }

    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        *self.state.lock().unwrap() = State::Stopped;
        Ok(())
    }

    fn query(&self, query: &mut gst::QueryRef) -> bool {
        use gst::QueryViewMut;
        match query.view_mut() {
            QueryViewMut::Scheduling(q) => {
                q.set(gst::SchedulingFlags::SEQUENTIAL | gst::SchedulingFlags::BANDWIDTH_LIMITED, 1, -1, 0);
                q.add_scheduling_modes([gst::PadMode::Push]);
                true
            }
            _ => BaseSrcImplExt::parent_query(self, query),
        }
    }

    /// Just moves the cursor - see the module docs on why this never
    /// touches the network. `segment.stop()` is intentionally ignored:
    /// every fetch is a whole fixed-size chunk regardless, and end of
    /// stream is governed by the discovered total size, not a per-seek
    /// upper bound (this element reads one continuous file, not
    /// adaptive-streaming byte ranges).
    fn do_seek(&self, segment: &mut gst::Segment) -> bool {
        let Some(segment) = segment.downcast_mut::<gst::format::Bytes>() else {
            return false;
        };

        let mut state = self.state.lock().unwrap();
        let State::Started { position, .. } = &mut *state else {
            gst::element_imp_error!(self, gst::LibraryError::Failed, ["Not started yet"]);
            return false;
        };

        let Some(start) = segment.start() else {
            return false;
        };
        info!(from = *position, to = *start, "upnpcachehttpsrc: do_seek()");
        *position = *start;
        true
    }
}

impl PushSrcImpl for CacheHttpSrc {
    fn create(&self, _buffer: Option<&mut gst::BufferRef>) -> Result<CreateSuccess, gst::FlowError> {
        let offset = match *self.state.lock().unwrap() {
            State::Started { position, .. } => position,
            State::Stopped => {
                gst::element_imp_error!(self, gst::LibraryError::Failed, ["Not started yet"]);
                return Err(gst::FlowError::Error);
            }
        };

        let data = match self.read_next() {
            Ok(Some(data)) => data,
            Ok(None) => {
                info!(offset, "upnpcachehttpsrc: create() -> Eos");
                return Err(gst::FlowError::Eos);
            }
            Err(Some(err)) => {
                warn!(offset, %err, "upnpcachehttpsrc: create() -> Error");
                self.post_error_message(err);
                return Err(gst::FlowError::Error);
            }
            Err(None) => {
                warn!(
                    offset,
                    "upnpcachehttpsrc: create() -> Flushing (cancelled/aborted read - if this repeats with \
                     no matching unlock()/unlock_stop() pair just before it, something is stuck)"
                );
                return Err(gst::FlowError::Flushing);
            }
        };

        let size = data.len() as u64;
        if let State::Started { position, .. } = &mut *self.state.lock().unwrap() {
            *position += size;
        }

        let mut buffer = gst::Buffer::from_slice(data);
        {
            let buffer = buffer.get_mut().unwrap();
            buffer.set_offset(offset);
            buffer.set_offset_end(offset + size);
        }

        Ok(CreateSuccess::NewBuffer(buffer))
    }
}

impl URIHandlerImpl for CacheHttpSrc {
    const URI_TYPE: gst::URIType = gst::URIType::Src;

    fn protocols() -> &'static [&'static str] {
        &["http", "https"]
    }

    fn uri(&self) -> Option<String> {
        self.settings.lock().unwrap().location.as_ref().map(Url::to_string)
    }

    fn set_uri(&self, uri: &str) -> Result<(), glib::Error> {
        self.set_location(Some(uri))
    }
}

#[glib::object_subclass]
impl ObjectSubclass for CacheHttpSrc {
    const NAME: &'static str = "GstUpnpCacheHttpSrc";
    type Type = super::CacheHttpSrc;
    type ParentType = gst_base::PushSrc;
    type Interfaces = (gst::URIHandler,);
}
