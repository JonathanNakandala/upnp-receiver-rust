//! [`MediaBackend`](super::MediaBackend) implementation built on the
//! system GStreamer install. `GStreamerBackend::new()` doubles as the
//! availability check: it fails if `gst::init()` can't run
//! which callers use to decide whether to fall
//! back to [`dummy`](super::dummy).

pub mod backend;
pub mod cache_src;
pub mod codecs;
pub mod video;

pub use backend::{GStreamerBackend, PlaybinVersion, SubtitleControl};
pub use codecs::supported_mime_types;
pub use video::{MenuState, PlaybackControls, VideoOutput};
