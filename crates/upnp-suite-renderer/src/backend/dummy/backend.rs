use super::error::DummyProcessError;
use super::manager::DummyBackendManager;
use crate::backend::{MediaBackend, MediaBackendError, TrackInfo};
use crate::server::soap::types::av_transport::{PlayMode, TransportState, TransportStatus};
use async_trait::async_trait;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
pub struct DummyBackend {
    manager: Arc<Mutex<DummyBackendManager>>,
    // The simulated backend never reaches end-of-stream on its own (see
    // `state_changes` below), so there's no gapless mechanism to promote
    // this to current — it's just remembered and reported back, for API
    // completeness with the real backend.
    next: Mutex<Option<TrackInfo>>,
    // No real decode pipeline to actually run at a different rate — just
    // remembered so a `set_play_speed(1.0)` (the only value accepted)
    // round-trips through `get_play_speed` for API completeness.
    speed: Mutex<f64>,
    // Same reasoning as `next`: no end-of-stream signal of its own to hook
    // a repeat loop into, so this is just remembered and reported back.
    play_mode: Mutex<PlayMode>,
    // No real mixer to drive — just remembered state, the same way
    // `GStreamerBackend` mirrors `playbin`'s `volume`/`mute` properties, so
    // RenderingControl works identically against either backend.
    volume: Mutex<u8>,
    muted: Mutex<bool>,
    // No video output to apply these to either — same reasoning as
    // `volume`/`muted`, just remembered for API completeness. 50 = neutral,
    // matching `GStreamerBackend`'s default. Kept as plain fields rather
    // than reusing `gstreamer::video::RenderParams` — this backend has no
    // GStreamer/video dependency at all, and shouldn't gain one just for a
    // struct shape.
    brightness: Mutex<u16>,
    contrast: Mutex<u16>,
    red_gain: Mutex<u16>,
    green_gain: Mutex<u16>,
    blue_gain: Mutex<u16>,
    red_black_level: Mutex<u16>,
    green_black_level: Mutex<u16>,
    blue_black_level: Mutex<u16>,
    color_temperature: Mutex<u16>,
    horizontal_keystone: Mutex<i16>,
    vertical_keystone: Mutex<i16>,
    sharpness: Mutex<u16>,
    // No audio EQ stage to apply this to either — same reasoning as
    // `volume`/`muted`, just remembered for API completeness.
    loudness: Mutex<bool>,
}

impl DummyBackend {
    pub fn new(manager: Arc<Mutex<DummyBackendManager>>) -> Self {
        Self {
            manager,
            next: Mutex::new(None),
            speed: Mutex::new(1.0),
            play_mode: Mutex::new(PlayMode::Normal),
            volume: Mutex::new(100),
            muted: Mutex::new(false),
            brightness: Mutex::new(50),
            contrast: Mutex::new(50),
            red_gain: Mutex::new(500),
            green_gain: Mutex::new(500),
            blue_gain: Mutex::new(500),
            red_black_level: Mutex::new(500),
            green_black_level: Mutex::new(500),
            blue_black_level: Mutex::new(500),
            color_temperature: Mutex::new(50),
            horizontal_keystone: Mutex::new(0),
            vertical_keystone: Mutex::new(0),
            sharpness: Mutex::new(0),
            loudness: Mutex::new(false),
        }
    }

    // Helper to convert error types
    fn convert_error(err: DummyProcessError) -> MediaBackendError {
        match err {
            DummyProcessError::NoConnection => {
                MediaBackendError::Connection("No connection to media process".to_string())
            }
            DummyProcessError::NoMediaLoaded => MediaBackendError::NoMediaLoaded,
            DummyProcessError::MaxRestartAttempts(attempts) => MediaBackendError::ProcessError(
                format!("Max restart attempts ({}) reached", attempts),
            ),
            DummyProcessError::Connection(msg) => MediaBackendError::Connection(msg),
            DummyProcessError::CommandFailed(msg) => MediaBackendError::Other(msg),
            DummyProcessError::UnexpectedResponse(msg) => {
                MediaBackendError::Other(format!("Unexpected response: {}", msg))
            }
            _ => MediaBackendError::Other(format!("Media process error: {:?}", err)),
        }
    }

    // Helper to convert playback states
    fn convert_playback_state(state: TransportState) -> TransportState {
        state
    }
}

#[async_trait]
impl MediaBackend for DummyBackend {
    async fn load_media(
        &self,
        uri: &str,
        metadata: Option<String>,
    ) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .load_uri(uri, metadata)
            .await
            .map_err(Self::convert_error)
    }

    async fn play(&self) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .play()
            .await
            .map_err(Self::convert_error)
    }

    async fn pause(&self) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .pause()
            .await
            .map_err(Self::convert_error)
    }

    async fn stop(&self) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .stop()
            .await
            .map_err(Self::convert_error)
    }

    async fn get_position(&self) -> Result<Duration, MediaBackendError> {
        self.manager
            .lock()
            .await
            .get_position()
            .await
            .map_err(Self::convert_error)
    }

    async fn seek(&self, position: Duration) -> Result<(), MediaBackendError> {
        self.manager
            .lock()
            .await
            .seek(position.as_millis() as u64)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_duration(&self) -> Result<Option<Duration>, MediaBackendError> {
        self.manager
            .lock()
            .await
            .get_duration()
            .await
            .map_err(Self::convert_error)
    }

    async fn get_playback_state(&self) -> Result<TransportState, MediaBackendError> {
        self.manager
            .lock()
            .await
            .get_state()
            .await
            .map(Self::convert_playback_state)
            .map_err(Self::convert_error)
    }

    async fn get_status(&self) -> Result<TransportStatus, MediaBackendError> {
        // The simulated backend never actually fails a playback attempt.
        Ok(TransportStatus::OK)
    }

    async fn set_play_speed(&self, speed: f64) -> Result<(), MediaBackendError> {
        if speed == 1.0 {
            *self.speed.lock().await = speed;
            Ok(())
        } else {
            Err(MediaBackendError::Unsupported(format!(
                "Playback speed control not yet implemented. Requested speed: {}",
                speed
            )))
        }
    }

    async fn get_play_speed(&self) -> Result<f64, MediaBackendError> {
        Ok(*self.speed.lock().await)
    }

    async fn set_play_mode(&self, mode: PlayMode) -> Result<(), MediaBackendError> {
        *self.play_mode.lock().await = mode;
        Ok(())
    }

    async fn get_play_mode(&self) -> Result<PlayMode, MediaBackendError> {
        Ok(*self.play_mode.lock().await)
    }

    async fn set_volume(&self, volume: u8) -> Result<(), MediaBackendError> {
        *self.volume.lock().await = volume;
        Ok(())
    }

    async fn get_volume(&self) -> Result<u8, MediaBackendError> {
        Ok(*self.volume.lock().await)
    }

    async fn set_mute(&self, muted: bool) -> Result<(), MediaBackendError> {
        *self.muted.lock().await = muted;
        Ok(())
    }

    async fn get_mute(&self) -> Result<bool, MediaBackendError> {
        Ok(*self.muted.lock().await)
    }

    async fn set_brightness(&self, brightness: u16) -> Result<(), MediaBackendError> {
        *self.brightness.lock().await = brightness;
        Ok(())
    }

    async fn get_brightness(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.brightness.lock().await)
    }

    async fn set_contrast(&self, contrast: u16) -> Result<(), MediaBackendError> {
        *self.contrast.lock().await = contrast;
        Ok(())
    }

    async fn get_contrast(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.contrast.lock().await)
    }

    async fn set_red_video_gain(&self, gain: u16) -> Result<(), MediaBackendError> {
        *self.red_gain.lock().await = gain;
        Ok(())
    }

    async fn get_red_video_gain(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.red_gain.lock().await)
    }

    async fn set_green_video_gain(&self, gain: u16) -> Result<(), MediaBackendError> {
        *self.green_gain.lock().await = gain;
        Ok(())
    }

    async fn get_green_video_gain(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.green_gain.lock().await)
    }

    async fn set_blue_video_gain(&self, gain: u16) -> Result<(), MediaBackendError> {
        *self.blue_gain.lock().await = gain;
        Ok(())
    }

    async fn get_blue_video_gain(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.blue_gain.lock().await)
    }

    async fn set_red_video_black_level(&self, level: u16) -> Result<(), MediaBackendError> {
        *self.red_black_level.lock().await = level;
        Ok(())
    }

    async fn get_red_video_black_level(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.red_black_level.lock().await)
    }

    async fn set_green_video_black_level(&self, level: u16) -> Result<(), MediaBackendError> {
        *self.green_black_level.lock().await = level;
        Ok(())
    }

    async fn get_green_video_black_level(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.green_black_level.lock().await)
    }

    async fn set_blue_video_black_level(&self, level: u16) -> Result<(), MediaBackendError> {
        *self.blue_black_level.lock().await = level;
        Ok(())
    }

    async fn get_blue_video_black_level(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.blue_black_level.lock().await)
    }

    async fn set_color_temperature(&self, temperature: u16) -> Result<(), MediaBackendError> {
        *self.color_temperature.lock().await = temperature;
        Ok(())
    }

    async fn get_color_temperature(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.color_temperature.lock().await)
    }

    async fn set_horizontal_keystone(&self, keystone: i16) -> Result<(), MediaBackendError> {
        *self.horizontal_keystone.lock().await = keystone;
        Ok(())
    }

    async fn get_horizontal_keystone(&self) -> Result<i16, MediaBackendError> {
        Ok(*self.horizontal_keystone.lock().await)
    }

    async fn set_vertical_keystone(&self, keystone: i16) -> Result<(), MediaBackendError> {
        *self.vertical_keystone.lock().await = keystone;
        Ok(())
    }

    async fn get_vertical_keystone(&self) -> Result<i16, MediaBackendError> {
        Ok(*self.vertical_keystone.lock().await)
    }

    async fn set_sharpness(&self, sharpness: u16) -> Result<(), MediaBackendError> {
        *self.sharpness.lock().await = sharpness;
        Ok(())
    }

    async fn get_sharpness(&self) -> Result<u16, MediaBackendError> {
        Ok(*self.sharpness.lock().await)
    }

    async fn set_loudness(&self, loudness: bool) -> Result<(), MediaBackendError> {
        *self.loudness.lock().await = loudness;
        Ok(())
    }

    async fn get_loudness(&self) -> Result<bool, MediaBackendError> {
        Ok(*self.loudness.lock().await)
    }

    async fn get_current_uri(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.manager.lock().await.get_uri().await)
    }

    async fn get_metadata(&self) -> Result<Option<String>, MediaBackendError> {
        Ok(self.manager.lock().await.get_current_metadata().await)
    }

    fn state_changes(&self) -> tokio::sync::watch::Receiver<TransportState> {
        // The simulated backend just loops its position counter back to 0
        // at the end of the (fake) duration rather than stopping — it never
        // has a state transition of its own to report, so the sender is
        // dropped immediately and this receiver never fires.
        let (_tx, rx) = tokio::sync::watch::channel(TransportState::NoMediaPresent);
        rx
    }

    async fn set_next_media(
        &self,
        uri: &str,
        metadata: Option<String>,
    ) -> Result<(), MediaBackendError> {
        *self.next.lock().await = Some(TrackInfo {
            uri: uri.to_string(),
            metadata,
        });
        Ok(())
    }

    async fn clear_next_media(&self) -> Result<(), MediaBackendError> {
        *self.next.lock().await = None;
        Ok(())
    }

    async fn get_next_media(&self) -> Result<Option<TrackInfo>, MediaBackendError> {
        Ok(self.next.lock().await.clone())
    }

    fn track_changes(&self) -> tokio::sync::watch::Receiver<TrackInfo> {
        // Never gaplessly promotes anything on its own (see `next` above),
        // so — same reasoning as `state_changes` — this never fires.
        let (_tx, rx) = tokio::sync::watch::channel(TrackInfo::default());
        rx
    }
}
