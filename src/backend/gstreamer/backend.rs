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

/// What we know about the currently loaded media that GStreamer itself
/// doesn't track for us (it only knows the `uri` property it was given, and
/// nothing about UPnP DIDL-Lite metadata at all).
#[derive(Default)]
struct LoadedMedia {
    uri: Option<String>,
    metadata: Option<String>,
}

pub struct GStreamerBackend {
    playbin: gst::Element,
    loaded: Mutex<LoadedMedia>,
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
            loaded: Mutex::new(LoadedMedia::default()),
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

        *self.loaded.lock().unwrap() = LoadedMedia {
            uri: Some(uri.to_string()),
            metadata,
        };

        // Pre-roll into Paused so playback can start immediately once Play
        // is called, without waiting on caps negotiation/buffering then.
        // set_state returns as soon as the change is *requested*, not once
        // it's actually complete (reading headers, negotiating caps, etc.
        // all happen async in GStreamer's own threads) — block (off the
        // async executor) until it actually finishes, so callers don't race
        // it.
        //
        // Note duration specifically is not guaranteed to be known at this
        // point even once PAUSED is reached: for formats without an
        // explicit duration header (plain MP3 without a Xing/VBRI frame,
        // for instance), GStreamer only resolves it once bytes are actually
        // flowing, i.e. some time after Play. get_duration() reflects that
        // honestly by returning None until GStreamer itself knows.
        self.playbin
            .set_state(gst::State::Paused)
            .map_err(state_change_error)?;

        let playbin = self.playbin.clone();
        tokio::task::spawn_blocking(move || {
            let (result, _, _) = playbin.state(gst::ClockTime::from_seconds(5));
            result.map_err(state_change_error)
        })
        .await
        .map_err(|e| MediaBackendError::GStreamer(format!("Preroll wait panicked: {}", e)))??;

        Ok(())
    }

    async fn play(&self) -> Result<(), MediaBackendError> {
        self.playbin
            .set_state(gst::State::Playing)
            .map_err(state_change_error)?;
        Ok(())
    }

    async fn pause(&self) -> Result<(), MediaBackendError> {
        self.playbin
            .set_state(gst::State::Paused)
            .map_err(state_change_error)?;
        Ok(())
    }

    async fn stop(&self) -> Result<(), MediaBackendError> {
        self.playbin
            .set_state(gst::State::Ready)
            .map_err(state_change_error)?;
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
        // Zero timeout: report the last state GStreamer told us about rather
        // than blocking this call on an in-flight async state change.
        let (_, current, _) = self.playbin.state(gst::ClockTime::ZERO);
        let has_media = self.loaded.lock().unwrap().uri.is_some();

        Ok(match (current, has_media) {
            (gst::State::Playing, _) => TransportState::Playing,
            (gst::State::Paused, _) => TransportState::PausedPlayback,
            (_, true) => TransportState::Stopped,
            (_, false) => TransportState::NoMediaPresent,
        })
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
        Ok(self.loaded.lock().unwrap().uri.clone())
    }

    async fn get_metadata(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.loaded.lock().unwrap().metadata.clone())
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
