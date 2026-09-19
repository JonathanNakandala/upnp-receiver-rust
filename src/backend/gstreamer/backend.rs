//! [`MediaBackend`] implementation on top of real GStreamer playback via
//! `playbin`, in-process — as opposed to [`DummyBackend`], which talks to a
//! separate simulated subprocess over TCP and does no real decoding.
//!
//! First step: get a `playbin` pipeline actually decoding and playing audio
//! (MP3 in particular — `playbin` auto-plugs the right demuxer/decoder for
//! whatever the source turns out to be, so this isn't MP3-specific beyond
//! "that's what's been exercised so far"). Volume/mute ride along for free
//! since `playbin` exposes them as plain properties.
//!
//! [`DummyBackend`]: crate::backend::dummy::DummyBackend

use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::backend::{MediaBackend, MediaBackendError};
use crate::server::soap::types::av_transport::TransportState;

/// What we track ourselves rather than asking GStreamer, either because it
/// doesn't know it at all (UPnP DIDL-Lite metadata) or because asking it
/// would mean reading a value that lags what we just told it to do (the
/// transport state — see [`GStreamerBackend::playbin`]).
struct PlayerState {
    uri: Option<String>,
    metadata: Option<String>,
    transport_state: TransportState,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            uri: None,
            metadata: None,
            transport_state: TransportState::NoMediaPresent,
        }
    }
}

pub struct GStreamerBackend {
    playbin: gst::Element,
    state: Mutex<PlayerState>,
}

impl GStreamerBackend {
    pub fn new() -> Result<Self, MediaBackendError> {
        ensure_initialized()?;

        let playbin = gst::ElementFactory::make("playbin")
            .name("upnp-receiver-playbin")
            .build()
            .map_err(|e| {
                MediaBackendError::GStreamer(format!("Failed to create playbin: {}", e))
            })?;

        Ok(Self {
            playbin,
            state: Mutex::new(PlayerState::default()),
        })
    }
}

impl Drop for GStreamerBackend {
    fn drop(&mut self) {
        // Best-effort: release the audio device and any decoder resources.
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

/// `gst::init()` must run once per process before any element is created,
/// and isn't itself safe to call concurrently from multiple backends.
fn ensure_initialized() -> Result<(), MediaBackendError> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    INIT.get_or_init(|| gst::init().map_err(|e| e.to_string()))
        .clone()
        .map_err(MediaBackendError::GStreamer)
}

fn state_change_error(err: gst::StateChangeError) -> MediaBackendError {
    MediaBackendError::GStreamer(format!("State change failed: {}", err))
}

#[async_trait]
impl MediaBackend for GStreamerBackend {
    async fn load_media(
        &self,
        uri: &str,
        metadata: Option<String>,
    ) -> Result<(), MediaBackendError> {
        // Drop back to Ready before switching URIs — playbin doesn't support
        // changing `uri` while past that state.
        self.playbin
            .set_state(gst::State::Ready)
            .map_err(state_change_error)?;

        self.playbin.set_property("uri", uri);

        {
            let mut state = self.state.lock().unwrap();
            state.uri = Some(uri.to_string());
            state.metadata = metadata;
            state.transport_state = TransportState::Stopped;
        }

        // Kick off preroll into Paused so playback can start immediately
        // once Play is called — fire-and-forget. `set_state` only requests
        // the transition; the actual work (caps negotiation, etc.) happens
        // asynchronously on GStreamer's own threads afterwards and can take
        // a little while. We deliberately don't block this call on it: the
        // `transport_state` we just set above (not a query back to
        // GStreamer) is what get_playback_state()/GetTransportInfo reports,
        // so callers see a prompt, consistent answer regardless of how far
        // GStreamer has actually gotten.
        //
        // Note duration specifically is not guaranteed to be known even
        // once PAUSED is reached: for formats without an explicit duration
        // header (plain MP3 without a Xing/VBRI frame, for instance),
        // GStreamer only resolves it once bytes are actually flowing, i.e.
        // some time after Play. get_duration() reflects that honestly by
        // returning None until GStreamer itself knows.
        self.playbin
            .set_state(gst::State::Paused)
            .map_err(state_change_error)?;

        Ok(())
    }

    async fn play(&self) -> Result<(), MediaBackendError> {
        self.playbin
            .set_state(gst::State::Playing)
            .map_err(state_change_error)?;
        self.state.lock().unwrap().transport_state = TransportState::Playing;
        Ok(())
    }

    async fn pause(&self) -> Result<(), MediaBackendError> {
        self.playbin
            .set_state(gst::State::Paused)
            .map_err(state_change_error)?;
        self.state.lock().unwrap().transport_state = TransportState::PausedPlayback;
        Ok(())
    }

    async fn stop(&self) -> Result<(), MediaBackendError> {
        self.playbin
            .set_state(gst::State::Ready)
            .map_err(state_change_error)?;
        let mut state = self.state.lock().unwrap();
        state.transport_state = if state.uri.is_some() {
            TransportState::Stopped
        } else {
            TransportState::NoMediaPresent
        };
        Ok(())
    }

    async fn get_position(&self) -> Result<Duration, MediaBackendError> {
        Ok(self
            .playbin
            .query_position::<gst::ClockTime>()
            .map(|t| Duration::from_nanos(t.nseconds()))
            .unwrap_or_default())
    }

    async fn seek(&self, position: Duration) -> Result<(), MediaBackendError> {
        let target = gst::ClockTime::from_nseconds(position.as_nanos() as u64);
        self.playbin
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, target)
            .map_err(|e| MediaBackendError::GStreamer(format!("Seek failed: {}", e)))
    }

    async fn get_duration(&self) -> Result<Option<Duration>, MediaBackendError> {
        Ok(self
            .playbin
            .query_duration::<gst::ClockTime>()
            .map(|t| Duration::from_nanos(t.nseconds())))
    }

    async fn get_playback_state(&self) -> Result<TransportState, MediaBackendError> {
        // The state we set the moment each command was accepted, not a
        // query back to GStreamer — its own internal state lags behind
        // whatever we last told it to do (see the comments in load_media,
        // play, pause and stop), so querying it here would report stale
        // values for a little while after every transition.
        Ok(self.state.lock().unwrap().transport_state)
    }

    async fn set_volume(&self, volume: u8) -> Result<(), MediaBackendError> {
        let linear = (volume as f64 / 100.0).clamp(0.0, 1.0);
        self.playbin.set_property("volume", linear);
        Ok(())
    }

    async fn get_volume(&self) -> Result<u8, MediaBackendError> {
        let linear: f64 = self.playbin.property("volume");
        Ok((linear.clamp(0.0, 1.0) * 100.0).round() as u8)
    }

    async fn set_mute(&self, muted: bool) -> Result<(), MediaBackendError> {
        self.playbin.set_property("mute", muted);
        Ok(())
    }

    async fn get_mute(&self) -> Result<bool, MediaBackendError> {
        Ok(self.playbin.property("mute"))
    }

    async fn get_current_uri(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.state.lock().unwrap().uri.clone())
    }

    async fn get_metadata(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.state.lock().unwrap().metadata.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a short sine-wave tone to a real MP3 file, entirely via
    /// GStreamer (no checked-in fixture, no external `gst-launch-1.0`
    /// process) — so the playback test below exercises a real MP3 decode,
    /// not just a container GStreamer happens to already understand.
    fn generate_test_mp3(path: &std::path::Path) {
        ensure_initialized().unwrap();

        let pipeline_desc = format!(
            "audiotestsrc num-buffers=50 wave=sine ! audioconvert ! lamemp3enc ! filesink location={}",
            path.display()
        );
        let pipeline = gst::parse::launch(&pipeline_desc)
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();

        pipeline.set_state(gst::State::Playing).unwrap();

        let bus = pipeline.bus().unwrap();
        for msg in bus.iter_timed(gst::ClockTime::NONE) {
            match msg.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(err) => panic!(
                    "test fixture encode pipeline error: {} ({:?})",
                    err.error(),
                    err.debug()
                ),
                _ => {}
            }
        }

        pipeline.set_state(gst::State::Null).unwrap();
    }

    #[tokio::test]
    #[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test -- --ignored`"]
    async fn plays_a_real_mp3_end_to_end() {
        let path =
            std::env::temp_dir().join(format!("upnp_receiver_test_{}.mp3", std::process::id()));
        generate_test_mp3(&path);
        let uri = format!("file://{}", path.display());

        let backend = GStreamerBackend::new().unwrap();
        backend
            .load_media(&uri, Some("test track".to_string()))
            .await
            .unwrap();

        backend.play().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(
            backend.get_playback_state().await.unwrap(),
            TransportState::Playing
        );

        // Our test fixture is a bare MP3 (no Xing/VBRI header), so duration
        // isn't resolved until bytes are actually flowing — check it here,
        // after Play, rather than right after load.
        let duration = backend.get_duration().await.unwrap();
        assert!(
            duration.is_some(),
            "playbin should know the duration once data is flowing"
        );

        let position = backend.get_position().await.unwrap();
        assert!(
            position > Duration::ZERO,
            "position should have advanced while playing, got {:?}",
            position
        );

        backend.pause().await.unwrap();
        assert_eq!(
            backend.get_playback_state().await.unwrap(),
            TransportState::PausedPlayback
        );

        backend.stop().await.unwrap();
        assert_eq!(
            backend.get_playback_state().await.unwrap(),
            TransportState::Stopped
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn reports_no_media_present_before_anything_is_loaded() {
        let backend = GStreamerBackend::new().unwrap();
        assert_eq!(
            backend.get_playback_state().await.unwrap(),
            TransportState::NoMediaPresent
        );
        assert_eq!(backend.get_current_uri().await.unwrap(), None);
    }

    #[tokio::test]
    async fn volume_and_mute_round_trip() {
        let backend = GStreamerBackend::new().unwrap();

        backend.set_volume(42).await.unwrap();
        assert_eq!(backend.get_volume().await.unwrap(), 42);

        backend.set_mute(true).await.unwrap();
        assert!(backend.get_mute().await.unwrap());

        backend.set_mute(false).await.unwrap();
        assert!(!backend.get_mute().await.unwrap());
    }
}
