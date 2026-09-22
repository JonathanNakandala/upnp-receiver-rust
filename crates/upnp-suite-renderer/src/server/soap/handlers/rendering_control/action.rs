use crate::server::eventing::{rendering_control_last_change, EventManager, RenderingControlEventState};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::rendering_control::{
    GetBlueVideoBlackLevelResponse, GetBlueVideoGainResponse, GetBrightnessResponse,
    GetColorTemperatureResponse, GetContrastResponse, GetGreenVideoBlackLevelResponse,
    GetGreenVideoGainResponse, GetHorizontalKeystoneResponse, GetMuteResponse,
    GetRedVideoBlackLevelResponse, GetRedVideoGainResponse, GetSharpnessResponse,
    GetVerticalKeystoneResponse, GetVolumeDBRangeResponse, GetVolumeDBResponse, GetVolumeResponse,
    ListPresetsResponse, RenderingControlAction, RenderingControlResponse, SelectPresetResponse,
    SetBlueVideoBlackLevelResponse, SetBlueVideoGainResponse, SetBrightnessResponse,
    SetColorTemperatureResponse, SetContrastResponse, SetGreenVideoBlackLevelResponse,
    SetGreenVideoGainResponse, SetHorizontalKeystoneResponse, SetMuteResponse,
    SetRedVideoBlackLevelResponse, SetRedVideoGainResponse, SetSharpnessResponse,
    SetVerticalKeystoneResponse, SetVolumeDBResponse, SetVolumeResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::instance::InstanceId;
use crate::services::rendering_control::RenderingControl;
use crate::services::MediaRenderer;
use std::sync::Arc;
use tracing::{info, warn};

/// Publishes a `LastChange` NOTIFY reflecting current picture/audio state
/// right after a state-changing action succeeds, so subscribed control
/// points find out immediately instead of waiting for their next poll.
/// `instance_id`/`control` are the instance the triggering action was
/// actually resolved against — see `RenderingControlAction::handle` below.
async fn publish_rendering_state(
    instance_id: InstanceId,
    control: &dyn RenderingControl,
    events: &Arc<EventManager>,
) {
    let volume = control.get_volume().await.unwrap_or(0);
    let muted = control.get_mute().await.unwrap_or(false);
    let brightness = control.get_brightness().await.unwrap_or(50);
    let contrast = control.get_contrast().await.unwrap_or(50);
    let red_gain = control.get_red_video_gain().await.unwrap_or(500);
    let green_gain = control.get_green_video_gain().await.unwrap_or(500);
    let blue_gain = control.get_blue_video_gain().await.unwrap_or(500);
    let red_black_level = control.get_red_video_black_level().await.unwrap_or(500);
    let green_black_level = control.get_green_video_black_level().await.unwrap_or(500);
    let blue_black_level = control.get_blue_video_black_level().await.unwrap_or(500);
    let color_temperature = control.get_color_temperature().await.unwrap_or(50);
    let horizontal_keystone = control.get_horizontal_keystone().await.unwrap_or(0);
    let vertical_keystone = control.get_vertical_keystone().await.unwrap_or(0);
    let sharpness = control.get_sharpness().await.unwrap_or(0);
    events.broadcast(rendering_control_last_change(
        instance_id,
        RenderingControlEventState {
            volume,
            muted,
            brightness,
            contrast,
            red_gain,
            green_gain,
            blue_gain,
            red_black_level,
            green_black_level,
            blue_black_level,
            color_temperature,
            horizontal_keystone,
            vertical_keystone,
            sharpness,
        },
    ));
}

/// UPnP's `A_ARG_TYPE_Channel` only ever allows `"Master"` here (see the
/// SCPD's `allowedValueList` for it) — this renderer has no concept of
/// individual audio channels to apply a per-channel setting to.
fn validate_channel(channel: &str) -> Result<(), SoapError> {
    if channel != "Master" {
        return Err(SoapError::Soap(SoapProtocolError::InvalidChannel(
            channel.to_string(),
        )));
    }
    Ok(())
}

impl UPnPAction for RenderingControlAction {
    type Response = RenderingControlResponse;

    fn action_name(&self) -> &'static str {
        match self {
            Self::ListPresets(_) => "ListPresets",
            Self::SelectPreset(_) => "SelectPreset",
            Self::SetVolume(_) => "SetVolume",
            Self::SetMute(_) => "SetMute",
            Self::GetVolume(_) => "GetVolume",
            Self::GetMute(_) => "GetMute",
            Self::SetVolumeDB(_) => "SetVolumeDB",
            Self::GetVolumeDB(_) => "GetVolumeDB",
            Self::GetVolumeDBRange(_) => "GetVolumeDBRange",
            Self::SetBrightness(_) => "SetBrightness",
            Self::GetBrightness(_) => "GetBrightness",
            Self::SetContrast(_) => "SetContrast",
            Self::GetContrast(_) => "GetContrast",
            Self::SetRedVideoGain(_) => "SetRedVideoGain",
            Self::GetRedVideoGain(_) => "GetRedVideoGain",
            Self::SetGreenVideoGain(_) => "SetGreenVideoGain",
            Self::GetGreenVideoGain(_) => "GetGreenVideoGain",
            Self::SetBlueVideoGain(_) => "SetBlueVideoGain",
            Self::GetBlueVideoGain(_) => "GetBlueVideoGain",
            Self::SetRedVideoBlackLevel(_) => "SetRedVideoBlackLevel",
            Self::GetRedVideoBlackLevel(_) => "GetRedVideoBlackLevel",
            Self::SetGreenVideoBlackLevel(_) => "SetGreenVideoBlackLevel",
            Self::GetGreenVideoBlackLevel(_) => "GetGreenVideoBlackLevel",
            Self::SetBlueVideoBlackLevel(_) => "SetBlueVideoBlackLevel",
            Self::GetBlueVideoBlackLevel(_) => "GetBlueVideoBlackLevel",
            Self::SetColorTemperature(_) => "SetColorTemperature",
            Self::GetColorTemperature(_) => "GetColorTemperature",
            Self::SetHorizontalKeystone(_) => "SetHorizontalKeystone",
            Self::GetHorizontalKeystone(_) => "GetHorizontalKeystone",
            Self::SetVerticalKeystone(_) => "SetVerticalKeystone",
            Self::GetVerticalKeystone(_) => "GetVerticalKeystone",
            Self::SetSharpness(_) => "SetSharpness",
            Self::GetSharpness(_) => "GetSharpness",
        }
    }

    fn namespace(&self) -> &'static str {
        "urn:schemas-upnp-org:service:RenderingControl:1"
    }

    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError> {
        // Resolved once per request — see AVTransportAction::handle for why
        // this happens before any action-specific logic, including for
        // ListPresets/SelectPreset below, which don't otherwise touch
        // `control` but still carry an InstanceID a control point could get
        // wrong.
        let instance_id = self.instance_id();
        let control = renderer.rendering_control.get(instance_id)?;

        match self {
            // We only ever expose one preset — there's no persisted
            // configuration to snapshot/restore beyond current volume/mute —
            // but the actions themselves are required by the RenderingControl
            // spec, so we implement them rather than leaving them 401.
            Self::ListPresets(req) => {
                info!(instance_id = req.instance_id, "Handling ListPresets");
                Ok(RenderingControlResponse::ListPresets(ListPresetsResponse {
                    current_preset_name_list: "FactoryDefaults".to_string(),
                }))
            }
            Self::SelectPreset(req) => {
                if req.preset_name != "FactoryDefaults" {
                    warn!(
                        instance_id = req.instance_id,
                        preset_name = %req.preset_name,
                        "Handling SelectPreset"
                    );
                    return Err(SoapError::Soap(SoapProtocolError::InvalidPresetName(
                        format!("Unknown preset: {}", req.preset_name),
                    )));
                }
                info!(
                    instance_id = req.instance_id,
                    preset_name = %req.preset_name,
                    "Handling SelectPreset"
                );
                Ok(RenderingControlResponse::SelectPreset(
                    SelectPresetResponse {},
                ))
            }
            Self::SetVolume(req) => {
                validate_channel(&req.channel)?;
                let result = control.set_volume(req.desired_volume).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_volume = req.desired_volume,
                        "Handling SetVolume"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_volume = req.desired_volume,
                        error = %err,
                        "Handling SetVolume"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetVolume(SetVolumeResponse {}))
            }
            Self::SetMute(req) => {
                validate_channel(&req.channel)?;
                let result = control.set_mute(req.desired_mute).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_mute = req.desired_mute,
                        "Handling SetMute"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_mute = req.desired_mute,
                        error = %err,
                        "Handling SetMute"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetMute(SetMuteResponse {}))
            }
            Self::GetVolume(req) => {
                validate_channel(&req.channel)?;
                let result = control.get_volume().await;
                match &result {
                    Ok(current_volume) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        current_volume,
                        "Handling GetVolume"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        error = %err,
                        "Handling GetVolume"
                    ),
                }
                let current_volume = result?;
                Ok(RenderingControlResponse::Volume(GetVolumeResponse {
                    current_volume,
                }))
            }
            Self::GetMute(req) => {
                validate_channel(&req.channel)?;
                let result = control.get_mute().await;
                match &result {
                    Ok(current_mute) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        current_mute,
                        "Handling GetMute"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        error = %err,
                        "Handling GetMute"
                    ),
                }
                let current_mute = result?;
                Ok(RenderingControlResponse::Mute(GetMuteResponse {
                    current_mute,
                }))
            }
            Self::SetVolumeDB(req) => {
                validate_channel(&req.channel)?;
                let result = control.set_volume_db(req.desired_volume).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_volume = req.desired_volume,
                        "Handling SetVolumeDB"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_volume = req.desired_volume,
                        error = %err,
                        "Handling SetVolumeDB"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetVolumeDB(SetVolumeDBResponse {}))
            }
            Self::GetVolumeDB(req) => {
                validate_channel(&req.channel)?;
                let result = control.get_volume_db().await;
                match &result {
                    Ok(current_volume) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        current_volume,
                        "Handling GetVolumeDB"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        error = %err,
                        "Handling GetVolumeDB"
                    ),
                }
                let current_volume = result?;
                Ok(RenderingControlResponse::VolumeDB(GetVolumeDBResponse {
                    current_volume,
                }))
            }
            Self::GetVolumeDBRange(req) => {
                validate_channel(&req.channel)?;
                let (min_value, max_value) = control.volume_db_range();
                info!(
                    instance_id = req.instance_id,
                    channel = %req.channel,
                    min_value,
                    max_value,
                    "Handling GetVolumeDBRange"
                );
                Ok(RenderingControlResponse::VolumeDBRange(
                    GetVolumeDBRangeResponse {
                        min_value,
                        max_value,
                    },
                ))
            }
            Self::SetBrightness(req) => {
                let result = control.set_brightness(req.desired_brightness).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_brightness = req.desired_brightness,
                        "Handling SetBrightness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_brightness = req.desired_brightness,
                        error = %err,
                        "Handling SetBrightness"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetBrightness(
                    SetBrightnessResponse {},
                ))
            }
            Self::GetBrightness(req) => {
                let result = control.get_brightness().await;
                match &result {
                    Ok(current_brightness) => info!(
                        instance_id = req.instance_id,
                        current_brightness,
                        "Handling GetBrightness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetBrightness"
                    ),
                }
                let current_brightness = result?;
                Ok(RenderingControlResponse::Brightness(
                    GetBrightnessResponse { current_brightness },
                ))
            }
            Self::SetContrast(req) => {
                let result = control.set_contrast(req.desired_contrast).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_contrast = req.desired_contrast,
                        "Handling SetContrast"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_contrast = req.desired_contrast,
                        error = %err,
                        "Handling SetContrast"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetContrast(
                    SetContrastResponse {},
                ))
            }
            Self::GetContrast(req) => {
                let result = control.get_contrast().await;
                match &result {
                    Ok(current_contrast) => info!(
                        instance_id = req.instance_id,
                        current_contrast,
                        "Handling GetContrast"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetContrast"
                    ),
                }
                let current_contrast = result?;
                Ok(RenderingControlResponse::Contrast(GetContrastResponse {
                    current_contrast,
                }))
            }
            Self::SetRedVideoGain(req) => {
                let result = control.set_red_video_gain(req.desired_red_video_gain).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_red_video_gain = req.desired_red_video_gain,
                        "Handling SetRedVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_red_video_gain = req.desired_red_video_gain,
                        error = %err,
                        "Handling SetRedVideoGain"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetRedVideoGain(
                    SetRedVideoGainResponse {},
                ))
            }
            Self::GetRedVideoGain(req) => {
                let result = control.get_red_video_gain().await;
                match &result {
                    Ok(current_red_video_gain) => info!(
                        instance_id = req.instance_id,
                        current_red_video_gain,
                        "Handling GetRedVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetRedVideoGain"
                    ),
                }
                let current_red_video_gain = result?;
                Ok(RenderingControlResponse::RedVideoGain(
                    GetRedVideoGainResponse {
                        current_red_video_gain,
                    },
                ))
            }
            Self::SetGreenVideoGain(req) => {
                let result = control
                    .set_green_video_gain(req.desired_green_video_gain)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_green_video_gain = req.desired_green_video_gain,
                        "Handling SetGreenVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_green_video_gain = req.desired_green_video_gain,
                        error = %err,
                        "Handling SetGreenVideoGain"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetGreenVideoGain(
                    SetGreenVideoGainResponse {},
                ))
            }
            Self::GetGreenVideoGain(req) => {
                let result = control.get_green_video_gain().await;
                match &result {
                    Ok(current_green_video_gain) => info!(
                        instance_id = req.instance_id,
                        current_green_video_gain,
                        "Handling GetGreenVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetGreenVideoGain"
                    ),
                }
                let current_green_video_gain = result?;
                Ok(RenderingControlResponse::GreenVideoGain(
                    GetGreenVideoGainResponse {
                        current_green_video_gain,
                    },
                ))
            }
            Self::SetBlueVideoGain(req) => {
                let result = control.set_blue_video_gain(req.desired_blue_video_gain).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_blue_video_gain = req.desired_blue_video_gain,
                        "Handling SetBlueVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_blue_video_gain = req.desired_blue_video_gain,
                        error = %err,
                        "Handling SetBlueVideoGain"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetBlueVideoGain(
                    SetBlueVideoGainResponse {},
                ))
            }
            Self::GetBlueVideoGain(req) => {
                let result = control.get_blue_video_gain().await;
                match &result {
                    Ok(current_blue_video_gain) => info!(
                        instance_id = req.instance_id,
                        current_blue_video_gain,
                        "Handling GetBlueVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetBlueVideoGain"
                    ),
                }
                let current_blue_video_gain = result?;
                Ok(RenderingControlResponse::BlueVideoGain(
                    GetBlueVideoGainResponse {
                        current_blue_video_gain,
                    },
                ))
            }
            Self::SetRedVideoBlackLevel(req) => {
                let result = control
                    .set_red_video_black_level(req.desired_red_video_black_level)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_red_video_black_level = req.desired_red_video_black_level,
                        "Handling SetRedVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_red_video_black_level = req.desired_red_video_black_level,
                        error = %err,
                        "Handling SetRedVideoBlackLevel"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetRedVideoBlackLevel(
                    SetRedVideoBlackLevelResponse {},
                ))
            }
            Self::GetRedVideoBlackLevel(req) => {
                let result = control.get_red_video_black_level().await;
                match &result {
                    Ok(current_red_video_black_level) => info!(
                        instance_id = req.instance_id,
                        current_red_video_black_level,
                        "Handling GetRedVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetRedVideoBlackLevel"
                    ),
                }
                let current_red_video_black_level = result?;
                Ok(RenderingControlResponse::RedVideoBlackLevel(
                    GetRedVideoBlackLevelResponse {
                        current_red_video_black_level,
                    },
                ))
            }
            Self::SetGreenVideoBlackLevel(req) => {
                let result = control
                    .set_green_video_black_level(req.desired_green_video_black_level)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_green_video_black_level = req.desired_green_video_black_level,
                        "Handling SetGreenVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_green_video_black_level = req.desired_green_video_black_level,
                        error = %err,
                        "Handling SetGreenVideoBlackLevel"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetGreenVideoBlackLevel(
                    SetGreenVideoBlackLevelResponse {},
                ))
            }
            Self::GetGreenVideoBlackLevel(req) => {
                let result = control.get_green_video_black_level().await;
                match &result {
                    Ok(current_green_video_black_level) => info!(
                        instance_id = req.instance_id,
                        current_green_video_black_level,
                        "Handling GetGreenVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetGreenVideoBlackLevel"
                    ),
                }
                let current_green_video_black_level = result?;
                Ok(RenderingControlResponse::GreenVideoBlackLevel(
                    GetGreenVideoBlackLevelResponse {
                        current_green_video_black_level,
                    },
                ))
            }
            Self::SetBlueVideoBlackLevel(req) => {
                let result = control
                    .set_blue_video_black_level(req.desired_blue_video_black_level)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_blue_video_black_level = req.desired_blue_video_black_level,
                        "Handling SetBlueVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_blue_video_black_level = req.desired_blue_video_black_level,
                        error = %err,
                        "Handling SetBlueVideoBlackLevel"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetBlueVideoBlackLevel(
                    SetBlueVideoBlackLevelResponse {},
                ))
            }
            Self::GetBlueVideoBlackLevel(req) => {
                let result = control.get_blue_video_black_level().await;
                match &result {
                    Ok(current_blue_video_black_level) => info!(
                        instance_id = req.instance_id,
                        current_blue_video_black_level,
                        "Handling GetBlueVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetBlueVideoBlackLevel"
                    ),
                }
                let current_blue_video_black_level = result?;
                Ok(RenderingControlResponse::BlueVideoBlackLevel(
                    GetBlueVideoBlackLevelResponse {
                        current_blue_video_black_level,
                    },
                ))
            }
            Self::SetColorTemperature(req) => {
                let result = control
                    .set_color_temperature(req.desired_color_temperature)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_color_temperature = req.desired_color_temperature,
                        "Handling SetColorTemperature"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_color_temperature = req.desired_color_temperature,
                        error = %err,
                        "Handling SetColorTemperature"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetColorTemperature(
                    SetColorTemperatureResponse {},
                ))
            }
            Self::GetColorTemperature(req) => {
                let result = control.get_color_temperature().await;
                match &result {
                    Ok(current_color_temperature) => info!(
                        instance_id = req.instance_id,
                        current_color_temperature,
                        "Handling GetColorTemperature"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetColorTemperature"
                    ),
                }
                let current_color_temperature = result?;
                Ok(RenderingControlResponse::ColorTemperature(
                    GetColorTemperatureResponse {
                        current_color_temperature,
                    },
                ))
            }
            Self::SetHorizontalKeystone(req) => {
                let result = control
                    .set_horizontal_keystone(req.desired_horizontal_keystone)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_horizontal_keystone = req.desired_horizontal_keystone,
                        "Handling SetHorizontalKeystone"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_horizontal_keystone = req.desired_horizontal_keystone,
                        error = %err,
                        "Handling SetHorizontalKeystone"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetHorizontalKeystone(
                    SetHorizontalKeystoneResponse {},
                ))
            }
            Self::GetHorizontalKeystone(req) => {
                let result = control.get_horizontal_keystone().await;
                match &result {
                    Ok(current_horizontal_keystone) => info!(
                        instance_id = req.instance_id,
                        current_horizontal_keystone,
                        "Handling GetHorizontalKeystone"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetHorizontalKeystone"
                    ),
                }
                let current_horizontal_keystone = result?;
                Ok(RenderingControlResponse::HorizontalKeystone(
                    GetHorizontalKeystoneResponse {
                        current_horizontal_keystone,
                    },
                ))
            }
            Self::SetVerticalKeystone(req) => {
                let result = control
                    .set_vertical_keystone(req.desired_vertical_keystone)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_vertical_keystone = req.desired_vertical_keystone,
                        "Handling SetVerticalKeystone"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_vertical_keystone = req.desired_vertical_keystone,
                        error = %err,
                        "Handling SetVerticalKeystone"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetVerticalKeystone(
                    SetVerticalKeystoneResponse {},
                ))
            }
            Self::GetVerticalKeystone(req) => {
                let result = control.get_vertical_keystone().await;
                match &result {
                    Ok(current_vertical_keystone) => info!(
                        instance_id = req.instance_id,
                        current_vertical_keystone,
                        "Handling GetVerticalKeystone"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetVerticalKeystone"
                    ),
                }
                let current_vertical_keystone = result?;
                Ok(RenderingControlResponse::VerticalKeystone(
                    GetVerticalKeystoneResponse {
                        current_vertical_keystone,
                    },
                ))
            }
            Self::SetSharpness(req) => {
                let result = control.set_sharpness(req.desired_sharpness).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_sharpness = req.desired_sharpness,
                        "Handling SetSharpness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_sharpness = req.desired_sharpness,
                        error = %err,
                        "Handling SetSharpness"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetSharpness(
                    SetSharpnessResponse {},
                ))
            }
            Self::GetSharpness(req) => {
                let result = control.get_sharpness().await;
                match &result {
                    Ok(current_sharpness) => info!(
                        instance_id = req.instance_id,
                        current_sharpness,
                        "Handling GetSharpness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetSharpness"
                    ),
                }
                let current_sharpness = result?;
                Ok(RenderingControlResponse::Sharpness(GetSharpnessResponse {
                    current_sharpness,
                }))
            }
        }
    }
}
