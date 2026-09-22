use crate::services::error::ServiceError;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error};

#[derive(Debug)]
pub struct RenderingState {
    volume: u16,
    mute: bool,
    brightness: u16,
    contrast: u16,
    red_gain: u16,
    green_gain: u16,
    blue_gain: u16,
    red_black_level: u16,
    green_black_level: u16,
    blue_black_level: u16,
    color_temperature: u16,
    horizontal_keystone: i16,
    vertical_keystone: i16,
}

impl Default for RenderingState {
    fn default() -> Self {
        Self {
            volume: 50, // Default to 50% volume
            mute: false,
            brightness: 50, // 50 = neutral/no adjustment
            contrast: 50,
            red_gain: 500, // 500 = neutral/no adjustment (0-1000 range)
            green_gain: 500,
            blue_gain: 500,
            red_black_level: 500,
            green_black_level: 500,
            blue_black_level: 500,
            color_temperature: 50, // 50 = neutral = D65 (6500K)
            horizontal_keystone: 0, // 0 = neutral (no warp)
            vertical_keystone: 0,
        }
    }
}

/// Shared range check, `0..=max`. `Brightness`/`Contrast`/`ColorTemperature`
/// use `max = 100` (a plain percent scale, 50 = neutral); the Gain/
/// BlackLevel pair uses `max = 1000` (500 = neutral) - see
/// `gstreamer::video::RenderParams`'s doc for why those need finer steps.
pub fn validate_range(value: u16, max: u16, name: &str) -> Result<(), ServiceError> {
    if value > max {
        return Err(ServiceError::InvalidValue(format!(
            "{name} must be between 0 and {max}, got {value}"
        )));
    }
    Ok(())
}

/// `validate_range(value, 100, name)` - kept as its own name since
/// `Brightness`/`Contrast`/`ColorTemperature`'s 0-100 percent scale is the
/// common case callers reach for.
pub fn validate_percent(value: u16, name: &str) -> Result<(), ServiceError> {
    validate_range(value, 100, name)
}

/// Signed counterpart to [`validate_range`] - `HorizontalKeystone`/
/// `VerticalKeystone`'s `i2` type and negative-capable range (the spec
/// requires their minimum be `<= 0`, unlike every other picture control
/// here) don't fit `validate_range`'s unsigned `0..=max`.
pub fn validate_signed_range(value: i16, min: i16, max: i16, name: &str) -> Result<(), ServiceError> {
    if value < min || value > max {
        return Err(ServiceError::InvalidValue(format!(
            "{name} must be between {min} and {max}, got {value}"
        )));
    }
    Ok(())
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

    /// UPnP's `Brightness`/`Contrast` - 0-100, 50 = neutral/no adjustment
    /// (see [`validate_percent`]). No default impl, unlike `get_volume_db`
    /// above: these aren't derived from another state variable, each
    /// implementor tracks (and, for a real video backend, applies) its own
    /// value.
    async fn set_brightness(&self, brightness: u16) -> Result<(), ServiceError>;
    async fn get_brightness(&self) -> Result<u16, ServiceError>;
    async fn set_contrast(&self, contrast: u16) -> Result<(), ServiceError>;
    async fn get_contrast(&self) -> Result<u16, ServiceError>;

    /// UPnP's 2-point white balance pair per primary color - `Output = Gain
    /// * Input + BlackLevel`. 0-1000, 500 = neutral for both (see
    /// [`validate_range`]).
    async fn set_red_video_gain(&self, gain: u16) -> Result<(), ServiceError>;
    async fn get_red_video_gain(&self) -> Result<u16, ServiceError>;
    async fn set_green_video_gain(&self, gain: u16) -> Result<(), ServiceError>;
    async fn get_green_video_gain(&self) -> Result<u16, ServiceError>;
    async fn set_blue_video_gain(&self, gain: u16) -> Result<(), ServiceError>;
    async fn get_blue_video_gain(&self) -> Result<u16, ServiceError>;
    async fn set_red_video_black_level(&self, level: u16) -> Result<(), ServiceError>;
    async fn get_red_video_black_level(&self) -> Result<u16, ServiceError>;
    async fn set_green_video_black_level(&self, level: u16) -> Result<(), ServiceError>;
    async fn get_green_video_black_level(&self) -> Result<u16, ServiceError>;
    async fn set_blue_video_black_level(&self, level: u16) -> Result<(), ServiceError>;
    async fn get_blue_video_black_level(&self) -> Result<u16, ServiceError>;

    /// UPnP's `ColorTemperature` - 0-100, 50 = neutral = D65 (6500K).
    async fn set_color_temperature(&self, temperature: u16) -> Result<(), ServiceError>;
    async fn get_color_temperature(&self) -> Result<u16, ServiceError>;

    /// UPnP's `HorizontalKeystone`/`VerticalKeystone` - -50..=50, 0 =
    /// neutral. A trapezoidal warp of the displayed shape, not a color
    /// adjustment - see [`validate_signed_range`].
    async fn set_horizontal_keystone(&self, keystone: i16) -> Result<(), ServiceError>;
    async fn get_horizontal_keystone(&self) -> Result<i16, ServiceError>;
    async fn set_vertical_keystone(&self, keystone: i16) -> Result<(), ServiceError>;
    async fn get_vertical_keystone(&self) -> Result<i16, ServiceError>;

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

    async fn set_brightness(&self, brightness: u16) -> Result<(), ServiceError> {
        validate_percent(brightness, "Brightness")?;
        debug!(brightness = brightness, "Setting brightness");
        self.state.write().await.brightness = brightness;
        Ok(())
    }

    async fn get_brightness(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.brightness)
    }

    async fn set_contrast(&self, contrast: u16) -> Result<(), ServiceError> {
        validate_percent(contrast, "Contrast")?;
        debug!(contrast = contrast, "Setting contrast");
        self.state.write().await.contrast = contrast;
        Ok(())
    }

    async fn get_contrast(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.contrast)
    }

    async fn set_red_video_gain(&self, gain: u16) -> Result<(), ServiceError> {
        validate_range(gain, 1000, "RedVideoGain")?;
        self.state.write().await.red_gain = gain;
        Ok(())
    }

    async fn get_red_video_gain(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.red_gain)
    }

    async fn set_green_video_gain(&self, gain: u16) -> Result<(), ServiceError> {
        validate_range(gain, 1000, "GreenVideoGain")?;
        self.state.write().await.green_gain = gain;
        Ok(())
    }

    async fn get_green_video_gain(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.green_gain)
    }

    async fn set_blue_video_gain(&self, gain: u16) -> Result<(), ServiceError> {
        validate_range(gain, 1000, "BlueVideoGain")?;
        self.state.write().await.blue_gain = gain;
        Ok(())
    }

    async fn get_blue_video_gain(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.blue_gain)
    }

    async fn set_red_video_black_level(&self, level: u16) -> Result<(), ServiceError> {
        validate_range(level, 1000, "RedVideoBlackLevel")?;
        self.state.write().await.red_black_level = level;
        Ok(())
    }

    async fn get_red_video_black_level(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.red_black_level)
    }

    async fn set_green_video_black_level(&self, level: u16) -> Result<(), ServiceError> {
        validate_range(level, 1000, "GreenVideoBlackLevel")?;
        self.state.write().await.green_black_level = level;
        Ok(())
    }

    async fn get_green_video_black_level(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.green_black_level)
    }

    async fn set_blue_video_black_level(&self, level: u16) -> Result<(), ServiceError> {
        validate_range(level, 1000, "BlueVideoBlackLevel")?;
        self.state.write().await.blue_black_level = level;
        Ok(())
    }

    async fn get_blue_video_black_level(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.blue_black_level)
    }

    async fn set_color_temperature(&self, temperature: u16) -> Result<(), ServiceError> {
        validate_percent(temperature, "ColorTemperature")?;
        self.state.write().await.color_temperature = temperature;
        Ok(())
    }

    async fn get_color_temperature(&self) -> Result<u16, ServiceError> {
        Ok(self.state.read().await.color_temperature)
    }

    async fn set_horizontal_keystone(&self, keystone: i16) -> Result<(), ServiceError> {
        validate_signed_range(keystone, -50, 50, "HorizontalKeystone")?;
        self.state.write().await.horizontal_keystone = keystone;
        Ok(())
    }

    async fn get_horizontal_keystone(&self) -> Result<i16, ServiceError> {
        Ok(self.state.read().await.horizontal_keystone)
    }

    async fn set_vertical_keystone(&self, keystone: i16) -> Result<(), ServiceError> {
        validate_signed_range(keystone, -50, 50, "VerticalKeystone")?;
        self.state.write().await.vertical_keystone = keystone;
        Ok(())
    }

    async fn get_vertical_keystone(&self) -> Result<i16, ServiceError> {
        Ok(self.state.read().await.vertical_keystone)
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

    #[tokio::test]
    async fn test_brightness_control() {
        let control = RenderingControlImpl::new();

        assert_eq!(control.get_brightness().await.unwrap(), 50);

        control.set_brightness(80).await.unwrap();
        assert_eq!(control.get_brightness().await.unwrap(), 80);

        assert!(control.set_brightness(101).await.is_err());
    }

    #[tokio::test]
    async fn test_contrast_control() {
        let control = RenderingControlImpl::new();

        assert_eq!(control.get_contrast().await.unwrap(), 50);

        control.set_contrast(20).await.unwrap();
        assert_eq!(control.get_contrast().await.unwrap(), 20);

        assert!(control.set_contrast(101).await.is_err());
    }

    #[tokio::test]
    async fn test_red_video_gain_control() {
        let control = RenderingControlImpl::new();

        assert_eq!(control.get_red_video_gain().await.unwrap(), 500);

        control.set_red_video_gain(750).await.unwrap();
        assert_eq!(control.get_red_video_gain().await.unwrap(), 750);

        assert!(control.set_red_video_gain(1001).await.is_err());
    }

    #[tokio::test]
    async fn test_green_video_gain_control() {
        let control = RenderingControlImpl::new();
        control.set_green_video_gain(0).await.unwrap();
        assert_eq!(control.get_green_video_gain().await.unwrap(), 0);
        assert!(control.set_green_video_gain(1001).await.is_err());
    }

    #[tokio::test]
    async fn test_blue_video_gain_control() {
        let control = RenderingControlImpl::new();
        control.set_blue_video_gain(1000).await.unwrap();
        assert_eq!(control.get_blue_video_gain().await.unwrap(), 1000);
        assert!(control.set_blue_video_gain(1001).await.is_err());
    }

    #[tokio::test]
    async fn test_red_video_black_level_control() {
        let control = RenderingControlImpl::new();

        assert_eq!(control.get_red_video_black_level().await.unwrap(), 500);

        control.set_red_video_black_level(300).await.unwrap();
        assert_eq!(control.get_red_video_black_level().await.unwrap(), 300);

        assert!(control.set_red_video_black_level(1001).await.is_err());
    }

    #[tokio::test]
    async fn test_green_video_black_level_control() {
        let control = RenderingControlImpl::new();
        control.set_green_video_black_level(300).await.unwrap();
        assert_eq!(control.get_green_video_black_level().await.unwrap(), 300);
        assert!(control.set_green_video_black_level(1001).await.is_err());
    }

    #[tokio::test]
    async fn test_blue_video_black_level_control() {
        let control = RenderingControlImpl::new();
        control.set_blue_video_black_level(300).await.unwrap();
        assert_eq!(control.get_blue_video_black_level().await.unwrap(), 300);
        assert!(control.set_blue_video_black_level(1001).await.is_err());
    }

    #[tokio::test]
    async fn test_color_temperature_control() {
        let control = RenderingControlImpl::new();

        // 50 = neutral = D65 (6500K) - see RenderParams::color_temperature's doc.
        assert_eq!(control.get_color_temperature().await.unwrap(), 50);

        control.set_color_temperature(0).await.unwrap();
        assert_eq!(control.get_color_temperature().await.unwrap(), 0);

        assert!(control.set_color_temperature(101).await.is_err());
    }

    #[tokio::test]
    async fn test_horizontal_keystone_control() {
        let control = RenderingControlImpl::new();

        assert_eq!(control.get_horizontal_keystone().await.unwrap(), 0);

        control.set_horizontal_keystone(-30).await.unwrap();
        assert_eq!(control.get_horizontal_keystone().await.unwrap(), -30);

        assert!(control.set_horizontal_keystone(-51).await.is_err());
        assert!(control.set_horizontal_keystone(51).await.is_err());
    }

    #[tokio::test]
    async fn test_vertical_keystone_control() {
        let control = RenderingControlImpl::new();

        assert_eq!(control.get_vertical_keystone().await.unwrap(), 0);

        control.set_vertical_keystone(50).await.unwrap();
        assert_eq!(control.get_vertical_keystone().await.unwrap(), 50);

        assert!(control.set_vertical_keystone(-51).await.is_err());
        assert!(control.set_vertical_keystone(51).await.is_err());
    }
}
