use crate::server::soap::error::SoapError;
use crate::server::soap::types::rendering_control::{
    GetMuteResponse, GetVolumeResponse, RenderingControlAction, RenderingControlResponse,
    SetMuteResponse, SetVolumeResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::MediaRenderer;
use std::sync::Arc;

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
                renderer
                    .rendering_control
                    .set_volume(req.desired_volume)
                    .await?;
                Ok(RenderingControlResponse::SetVolume(SetVolumeResponse {}))
            }
            Self::SetMute(req) => {
                renderer.rendering_control.set_mute(req.desired_mute).await?;
                Ok(RenderingControlResponse::SetMute(SetMuteResponse {}))
            }
            Self::GetVolume(_) => {
                let current_volume = renderer.rendering_control.get_volume().await?;
                Ok(RenderingControlResponse::Volume(GetVolumeResponse {
                    current_volume,
                }))
            }
            Self::GetMute(_) => {
                let current_mute = renderer.rendering_control.get_mute().await?;
                Ok(RenderingControlResponse::Mute(GetMuteResponse {
                    current_mute,
                }))
            }
        }
    }
}
