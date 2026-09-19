use crate::server::eventing::{rendering_control_last_change, EventManager};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::rendering_control::{
    GetMuteResponse, GetVolumeDBRangeResponse, GetVolumeDBResponse, GetVolumeResponse,
    ListPresetsResponse, RenderingControlAction, RenderingControlResponse, SelectPresetResponse,
    SetMuteResponse, SetVolumeDBResponse, SetVolumeResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::MediaRenderer;
use std::sync::Arc;
use tracing::{info, warn};

/// Publishes a `LastChange` NOTIFY reflecting current volume/mute right
/// after a state-changing action succeeds, so subscribed control points
/// find out immediately instead of waiting for their next poll.
async fn publish_rendering_state(renderer: &MediaRenderer, events: &Arc<EventManager>) {
    let volume = renderer.rendering_control.get_volume().await.unwrap_or(0);
    let muted = renderer
        .rendering_control
        .get_mute()
        .await
        .unwrap_or(false);
    events.broadcast(rendering_control_last_change(volume, muted));
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
        }
    }

    fn namespace(&self) -> &'static str {
        "urn:schemas-upnp-org:service:RenderingControl:1"
    }

    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError> {
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
                let result = renderer
                    .rendering_control
                    .set_volume(req.desired_volume)
                    .await;
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
                publish_rendering_state(&renderer, &renderer.rendering_control_events).await;
                Ok(RenderingControlResponse::SetVolume(SetVolumeResponse {}))
            }
            Self::SetMute(req) => {
                let result = renderer.rendering_control.set_mute(req.desired_mute).await;
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
                publish_rendering_state(&renderer, &renderer.rendering_control_events).await;
                Ok(RenderingControlResponse::SetMute(SetMuteResponse {}))
            }
            Self::GetVolume(req) => {
                let result = renderer.rendering_control.get_volume().await;
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
                let result = renderer.rendering_control.get_mute().await;
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
                let result = renderer
                    .rendering_control
                    .set_volume_db(req.desired_volume)
                    .await;
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
                publish_rendering_state(&renderer, &renderer.rendering_control_events).await;
                Ok(RenderingControlResponse::SetVolumeDB(SetVolumeDBResponse {}))
            }
            Self::GetVolumeDB(req) => {
                let result = renderer.rendering_control.get_volume_db().await;
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
                let (min_value, max_value) = renderer.rendering_control.volume_db_range();
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
        }
    }
}
