//! Real [`MediaBackend`](super::MediaBackend) implementation built on the
//! system GStreamer install. `GStreamerBackend::new()` doubles as the
//! availability check: it fails if `gst::init()` can't run or the `playbin`
//! element isn't available, which callers use to decide whether to fall
//! back to [`dummy`](super::dummy).

pub mod backend;
pub mod codecs;

pub use backend::GStreamerBackend;
pub use codecs::supported_mime_types;
