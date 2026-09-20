//! Shared setup for tests that need a real, decodable audio file and a real
//! GStreamer pipeline — the GStreamer playback/performance suites, not the
//! HTTP/SOAP-level tests elsewhere in `common`.

use gstreamer::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `gst::init()` must run once per process before any element is created,
/// and isn't safe to call concurrently — tests in the same binary run on
/// multiple threads by default, so this needs the same guard
/// `GStreamerBackend::new()` uses internally, not just a bare `gst::init()`
/// call in every test.
pub fn ensure_gstreamer_initialized() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        gstreamer::init().expect("gstreamer::init should succeed on a machine with GStreamer installed");
    });
}

/// A fresh temp-file path for a test fixture, namespaced by a caller-chosen
/// name plus the process id so parallel test binaries never collide.
pub fn fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "upnp_receiver_test_{}_{}.mp3",
        name,
        std::process::id()
    ))
}

/// Encodes a sine-wave tone to a real MP3 file, entirely via GStreamer (no
/// checked-in fixture, no external `gst-launch-1.0` process) — so tests
/// exercise a real MP3 decode, not just a container GStreamer happens to
/// already understand. `num_buffers` controls the fixture's length: each
/// `audiotestsrc` buffer is ~23ms at the default sample rate, so 50 buffers
/// is a ~1s clip (enough for most correctness checks) and 500 is ~11.5s
/// (enough room to seek around in for latency tests).
pub fn generate_test_mp3(path: &Path, num_buffers: u32) {
    ensure_gstreamer_initialized();

    let pipeline_desc = format!(
        "audiotestsrc num-buffers={} wave=sine ! audioconvert ! lamemp3enc ! filesink location={}",
        num_buffers,
        path.display()
    );
    let pipeline = gstreamer::parse::launch(&pipeline_desc)
        .unwrap()
        .downcast::<gstreamer::Pipeline>()
        .unwrap();

    pipeline.set_state(gstreamer::State::Playing).unwrap();

    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match msg.view() {
            gstreamer::MessageView::Eos(_) => break,
            gstreamer::MessageView::Error(err) => panic!(
                "test fixture encode pipeline error: {} ({:?})",
                err.error(),
                err.debug()
            ),
            _ => {}
        }
    }

    pipeline.set_state(gstreamer::State::Null).unwrap();
}
