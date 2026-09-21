use crate::server::eventing::{rendering_control_last_change, EventManager};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::rendering_control::{
    GetBrightnessResponse, GetContrastResponse, GetMuteResponse, GetVolumeDBRangeResponse,
    GetVolumeDBResponse, GetVolumeResponse, ListPresetsResponse, RenderingControlAction,
    RenderingControlResponse, SelectPresetResponse, SetBrightnessResponse, SetContrastResponse,
    SetMuteResponse, SetVolumeDBResponse, SetVolumeResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::instance::InstanceId;
use crate::services::rendering_control::RenderingControl;
use crate::services::MediaRenderer;
use std::sync::Arc;
use tracing::{info, warn};

/// Publishes a `LastChange` NOTIFY reflecting current volume/mute/
/// brightness/contrast right after a state-changing action succeeds, so
/// subscribed control points find out immediately instead of waiting for
/// their next poll. `instance_id`/`control` are the instance the triggering
/// action was actually resolved against — see `RenderingControlAction::handle`
/// below.
async fn publish_rendering_state(
    instance_id: InstanceId,
    control: &dyn RenderingControl,
    events: &Arc<EventManager>,
) {
    let volume = control.get_volume().await.unwrap_or(0);
    let muted = control.get_mute().await.unwrap_or(false);
    let brightness = control.get_brightness().await.unwrap_or(50);
    let contrast = control.get_contrast().await.unwrap_or(50);
    events.broadcast(rendering_control_last_change(
        instance_id,
        volume,
        muted,
        brightness,
        contrast,
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
        }
    }
}
