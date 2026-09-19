use crate::server::eventing::{rendering_control_last_change, EventManager};
use crate::server::soap::error::SoapError;
use crate::server::soap::types::rendering_control::{
    GetMuteResponse, GetVolumeResponse, RenderingControlAction, RenderingControlResponse,
    SetMuteResponse, SetVolumeResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::MediaRenderer;
use std::sync::Arc;
use tracing::info;

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
            Self::SetVolume(_) => "SetVolume",
            Self::SetMute(_) => "SetMute",
            Self::GetVolume(_) => "GetVolume",
            Self::GetMute(_) => "GetMute",
        }
    }

    fn namespace(&self) -> &'static str {
        "urn:schemas-upnp-org:service:RenderingControl:1"
    }

    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError> {
        match self {
            Self::SetVolume(req) => {
                info!(
                    instance_id = req.instance_id,
                    channel = %req.channel,
                    desired_volume = req.desired_volume,
                    "Handling SetVolume"
                );
                renderer
                    .rendering_control
                    .set_volume(req.desired_volume)
                    .await?;
                publish_rendering_state(&renderer, &renderer.rendering_control_events).await;
                Ok(RenderingControlResponse::SetVolume(SetVolumeResponse {}))
            }
            Self::SetMute(req) => {
                info!(
                    instance_id = req.instance_id,
                    channel = %req.channel,
                    desired_mute = req.desired_mute,
                    "Handling SetMute"
                );
                renderer.rendering_control.set_mute(req.desired_mute).await?;
                publish_rendering_state(&renderer, &renderer.rendering_control_events).await;
                Ok(RenderingControlResponse::SetMute(SetMuteResponse {}))
            }
            Self::GetVolume(req) => {
                info!(instance_id = req.instance_id, channel = %req.channel, "Handling GetVolume");
                let current_volume = renderer.rendering_control.get_volume().await?;
                Ok(RenderingControlResponse::Volume(GetVolumeResponse {
                    current_volume,
                }))
            }
            Self::GetMute(req) => {
                info!(instance_id = req.instance_id, channel = %req.channel, "Handling GetMute");
                let current_mute = renderer.rendering_control.get_mute().await?;
                Ok(RenderingControlResponse::Mute(GetMuteResponse {
                    current_mute,
                }))
            }
        }
    }
}
