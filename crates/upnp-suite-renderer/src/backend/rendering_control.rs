use crate::services::error::{ConnectionError, ServiceError};
use crate::services::rendering_control::{
    validate_percent, validate_range, validate_signed_range, RenderingControl,
};
use async_trait::async_trait;
use std::sync::Arc;

use super::{MediaBackend, MediaBackendError};

pub struct MediaBackendRenderingControl {
    backend: Arc<dyn MediaBackend>,
}

impl MediaBackendRenderingControl {
    pub fn new(backend: Arc<dyn MediaBackend>) -> Self {
        Self { backend }
    }

    fn convert_error(err: MediaBackendError) -> ServiceError {
        match err {
            MediaBackendError::Unsupported(msg) => ServiceError::InvalidValue(msg),
            MediaBackendError::Connection(msg) => {
                ServiceError::Connection(ConnectionError::ConnectionFailed(msg))
            }
            other => ServiceError::Network(other.to_string()),
        }
    }

    // UPnP's Volume state variable is a ui2, but we only ever deal in
    // percent (0-100)
    fn validate_volume(volume: u16) -> Result<(), ServiceError> {
        if volume > 100 {
            return Err(ServiceError::InvalidValue(format!(
                "Volume must be between 0 and 100, got {}",
                volume
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl RenderingControl for MediaBackendRenderingControl {
    async fn set_volume(&self, volume: u16) -> Result<(), ServiceError> {
        Self::validate_volume(volume)?;
        self.backend
            .set_volume(volume as u8)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_volume(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_volume()
            .await
            .map(|v| v as u16)
            .map_err(Self::convert_error)
    }

    async fn set_mute(&self, mute: bool) -> Result<(), ServiceError> {
        self.backend
            .set_mute(mute)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_mute(&self) -> Result<bool, ServiceError> {
        self.backend.get_mute().await.map_err(Self::convert_error)
    }

    async fn set_brightness(&self, brightness: u16) -> Result<(), ServiceError> {
        validate_percent(brightness, "Brightness")?;
        self.backend
            .set_brightness(brightness)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_brightness(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_brightness()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_contrast(&self, contrast: u16) -> Result<(), ServiceError> {
        validate_percent(contrast, "Contrast")?;
        self.backend
            .set_contrast(contrast)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_contrast(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_contrast()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_red_video_gain(&self, gain: u16) -> Result<(), ServiceError> {
        validate_range(gain, 1000, "RedVideoGain")?;
        self.backend
            .set_red_video_gain(gain)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_red_video_gain(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_red_video_gain()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_green_video_gain(&self, gain: u16) -> Result<(), ServiceError> {
        validate_range(gain, 1000, "GreenVideoGain")?;
        self.backend
            .set_green_video_gain(gain)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_green_video_gain(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_green_video_gain()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_blue_video_gain(&self, gain: u16) -> Result<(), ServiceError> {
        validate_range(gain, 1000, "BlueVideoGain")?;
        self.backend
            .set_blue_video_gain(gain)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_blue_video_gain(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_blue_video_gain()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_red_video_black_level(&self, level: u16) -> Result<(), ServiceError> {
        validate_range(level, 1000, "RedVideoBlackLevel")?;
        self.backend
            .set_red_video_black_level(level)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_red_video_black_level(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_red_video_black_level()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_green_video_black_level(&self, level: u16) -> Result<(), ServiceError> {
        validate_range(level, 1000, "GreenVideoBlackLevel")?;
        self.backend
            .set_green_video_black_level(level)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_green_video_black_level(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_green_video_black_level()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_blue_video_black_level(&self, level: u16) -> Result<(), ServiceError> {
        validate_range(level, 1000, "BlueVideoBlackLevel")?;
        self.backend
            .set_blue_video_black_level(level)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_blue_video_black_level(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_blue_video_black_level()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_color_temperature(&self, temperature: u16) -> Result<(), ServiceError> {
        validate_percent(temperature, "ColorTemperature")?;
        self.backend
            .set_color_temperature(temperature)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_color_temperature(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_color_temperature()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_horizontal_keystone(&self, keystone: i16) -> Result<(), ServiceError> {
        validate_signed_range(keystone, -50, 50, "HorizontalKeystone")?;
        self.backend
            .set_horizontal_keystone(keystone)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_horizontal_keystone(&self) -> Result<i16, ServiceError> {
        self.backend
            .get_horizontal_keystone()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_vertical_keystone(&self, keystone: i16) -> Result<(), ServiceError> {
        validate_signed_range(keystone, -50, 50, "VerticalKeystone")?;
        self.backend
            .set_vertical_keystone(keystone)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_vertical_keystone(&self) -> Result<i16, ServiceError> {
        self.backend
            .get_vertical_keystone()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_sharpness(&self, sharpness: u16) -> Result<(), ServiceError> {
        validate_range(sharpness, 100, "Sharpness")?;
        self.backend
            .set_sharpness(sharpness)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_sharpness(&self) -> Result<u16, ServiceError> {
        self.backend
            .get_sharpness()
            .await
            .map_err(Self::convert_error)
    }

    async fn set_loudness(&self, loudness: bool) -> Result<(), ServiceError> {
        self.backend
            .set_loudness(loudness)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_loudness(&self) -> Result<bool, ServiceError> {
        self.backend
            .get_loudness()
            .await
            .map_err(Self::convert_error)
    }
}
