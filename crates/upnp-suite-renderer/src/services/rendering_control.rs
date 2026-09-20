use crate::services::error::ServiceError;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error};

#[derive(Debug)]
pub struct RenderingState {
    volume: u16,
    mute: bool,
}

impl Default for RenderingState {
    fn default() -> Self {
        Self {
            volume: 50, // Default to 50% volume
            mute: false,
        }
    }
}

// UPnP's `VolumeDB` has no spec-mandated scale, and implementations vary
// (some use centibels). This renders it as plain dB — the convention
// gmrender-resurrect and other GStreamer-based UPnP renderers use, since
// that's the closest precedent for a GStreamer-backed renderer like this
// one. -60dB is an arbitrary but conventional "near-silent" floor standing
// in for Volume=0's true -infinity dB; 0dB (unity gain) is Volume=100,
// since nothing here amplifies above unity.
pub const VOLUME_DB_MIN: i16 = -60;
pub const VOLUME_DB_MAX: i16 = 0;

/// `Volume` (0-100 linear percent) -> `VolumeDB`, via the standard
/// `dB = 20*log10(amplitude)` conversion — the same formula GStreamer's own
/// `playbin` "volume" property (also linear) uses internally, so this
/// reports what's actually happening to the signal rather than an
/// independent number.
pub fn volume_to_db(volume: u16) -> i16 {
    if volume == 0 {
        return VOLUME_DB_MIN;
    }
    let db = 20.0 * (volume as f64 / 100.0).log10();
    db.max(VOLUME_DB_MIN as f64).round() as i16
}

/// The inverse of [`volume_to_db`].
pub fn db_to_volume(db: i16) -> u16 {
    if db <= VOLUME_DB_MIN {
        return 0;
    }
    let linear = 10f64.powf(db as f64 / 20.0);
    (linear * 100.0).round().clamp(0.0, 100.0) as u16
}

#[async_trait]
pub trait RenderingControl: Send + Sync {
    async fn set_volume(&self, volume: u16) -> Result<(), ServiceError>;
    async fn get_volume(&self) -> Result<u16, ServiceError>;
    async fn set_mute(&self, mute: bool) -> Result<(), ServiceError>;
    async fn get_mute(&self) -> Result<bool, ServiceError>;

    /// UPnP's `GetVolumeDB` — derived from `get_volume` rather than tracked
    /// separately, so it can never drift from `Volume`/whatever the backend
    /// actually applies. See [`volume_to_db`].
    async fn get_volume_db(&self) -> Result<i16, ServiceError> {
        Ok(volume_to_db(self.get_volume().await?))
    }

    /// UPnP's `SetVolumeDB` — converts to `Volume` and calls `set_volume`,
    /// same reasoning as `get_volume_db`.
    async fn set_volume_db(&self, db: i16) -> Result<(), ServiceError> {
        if !(VOLUME_DB_MIN..=VOLUME_DB_MAX).contains(&db) {
            return Err(ServiceError::InvalidValue(format!(
                "VolumeDB must be between {} and {}, got {}",
                VOLUME_DB_MIN, VOLUME_DB_MAX, db
            )));
        }
        self.set_volume(db_to_volume(db)).await
    }

    /// UPnP's `GetVolumeDBRange` — static for every implementation (see
    /// [`VOLUME_DB_MIN`]/[`VOLUME_DB_MAX`]), so there's nothing per-backend
    /// to override here.
    fn volume_db_range(&self) -> (i16, i16) {
        (VOLUME_DB_MIN, VOLUME_DB_MAX)
    }
}

pub struct RenderingControlImpl {
    state: Arc<RwLock<RenderingState>>,
}

impl Default for RenderingControlImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderingControlImpl {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RenderingState::default())),
        }
    }

    fn validate_volume(volume: u16) -> Result<(), ServiceError> {
        if volume > 100 {
            error!(volume = volume, "Invalid volume value");
            return Err(ServiceError::InvalidValue(format!(
                "Volume must be between 0 and 100, got {}",
                volume
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl RenderingControl for RenderingControlImpl {
    async fn set_volume(&self, volume: u16) -> Result<(), ServiceError> {
        Self::validate_volume(volume)?;
        debug!(volume = volume, "Setting volume");

        let mut state = self.state.write().await;
        state.volume = volume;
        Ok(())
    }

    async fn get_volume(&self) -> Result<u16, ServiceError> {
        let state = self.state.read().await;
        Ok(state.volume)
    }

    async fn set_mute(&self, mute: bool) -> Result<(), ServiceError> {
        debug!(mute = mute, "Setting mute state");
        let mut state = self.state.write().await;
        state.mute = mute;
        Ok(())
    }

    async fn get_mute(&self) -> Result<bool, ServiceError> {
        let state = self.state.read().await;
        Ok(state.mute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_volume_control() {
        let control = RenderingControlImpl::new();

        // Test initial volume
        assert_eq!(control.get_volume().await.unwrap(), 50);

        // Test setting volume
        control.set_volume(75).await.unwrap();
        assert_eq!(control.get_volume().await.unwrap(), 75);

        // Test invalid volume
        assert!(control.set_volume(101).await.is_err());
    }

    #[tokio::test]
    async fn test_mute_control() {
        let control = RenderingControlImpl::new();

        // Test initial mute state
        assert!(!control.get_mute().await.unwrap());

        // Test setting mute
        control.set_mute(true).await.unwrap();
        assert!(control.get_mute().await.unwrap());

        // Test toggling mute
        control.set_mute(false).await.unwrap();
        assert!(!control.get_mute().await.unwrap());
    }

    #[test]
    fn volume_db_conversion_hits_the_endpoints() {
        assert_eq!(volume_to_db(100), 0);
        assert_eq!(volume_to_db(0), VOLUME_DB_MIN);
        assert_eq!(db_to_volume(0), 100);
        assert_eq!(db_to_volume(VOLUME_DB_MIN), 0);
    }

    #[test]
    fn volume_db_conversion_round_trips_approximately() {
        for volume in [1u16, 10, 25, 50, 75, 90, 100] {
            let db = volume_to_db(volume);
            assert!(
                (VOLUME_DB_MIN..=VOLUME_DB_MAX).contains(&db),
                "db out of range for volume {volume}: {db}"
            );
            let round_tripped = db_to_volume(db);
            // Rounding to a whole dB is lossy — near volume=75 a single dB
            // of rounding error is already a ~5-point swing in linear
            // percent, since the log curve is steepest in the upper range.
            // This just guards against a broken formula, not exact
            // round-trip precision.
            assert!(
                (round_tripped as i32 - volume as i32).abs() <= 6,
                "volume {volume} -> {db}dB -> {round_tripped} drifted too much"
            );
        }
    }

    #[tokio::test]
    async fn get_volume_db_matches_current_volume() {
        let control = RenderingControlImpl::new();
        control.set_volume(50).await.unwrap();
        assert_eq!(control.get_volume_db().await.unwrap(), volume_to_db(50));
    }

    #[tokio::test]
    async fn set_volume_db_updates_volume() {
        let control = RenderingControlImpl::new();
        control.set_volume_db(0).await.unwrap();
        assert_eq!(control.get_volume().await.unwrap(), 100);
    }

    #[tokio::test]
    async fn set_volume_db_rejects_out_of_range() {
        let control = RenderingControlImpl::new();
        assert!(control.set_volume_db(VOLUME_DB_MIN - 1).await.is_err());
        assert!(control.set_volume_db(VOLUME_DB_MAX + 1).await.is_err());
    }

    #[test]
    fn volume_db_range_is_static() {
        let control = RenderingControlImpl::new();
        assert_eq!(control.volume_db_range(), (VOLUME_DB_MIN, VOLUME_DB_MAX));
    }
}
