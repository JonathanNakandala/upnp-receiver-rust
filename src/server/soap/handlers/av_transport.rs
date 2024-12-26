use crate::server::soap::action::SoapAction;
use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::av_transport::{
    AVTransportResponse, GetMediaInfoRequest, GetPositionInfoRequest, GetPositionInfoResponse,
    GetTransportInfoRequest, GetTransportInfoResponse, PauseRequest, PlayRequest,
    SetAVTransportURIRequest, StopRequest,
};
use crate::server::soap::types::soap_response::EmptyResponse;
use crate::server::soap::types::{UPnPAction, UPnPService};
use crate::server::soap::utils::create_soap_response;
use crate::services::MediaRenderer;
use async_trait::async_trait;
use axum::body::Body;
use std::sync::Arc;

use axum::http::Response;
use axum::response::IntoResponse;
use serde::Deserialize;
use tracing::debug;

#[derive(Debug, Deserialize)]
#[serde(tag = "Action")]
#[serde(rename_all = "PascalCase")]
pub enum AVTransportAction {
    #[serde(rename = "u:SetAVTransportURI")]
    SetAVTransportURI(SetAVTransportURIRequest),
    #[serde(rename = "u:Play")]
    Play(PlayRequest),
    #[serde(rename = "u:Pause")]
    Pause(PauseRequest),
    #[serde(rename = "u:Stop")]
    Stop(StopRequest),
    #[serde(rename = "u:GetTransportInfo")]
    GetTransportInfo(GetTransportInfoRequest),
    #[serde(rename = "u:GetMediaInfo")]
    GetMediaInfo(GetMediaInfoRequest),
    #[serde(rename = "u:GetPositionInfo")]
    GetPositionInfo(GetPositionInfoRequest),
}

impl UPnPAction for AVTransportAction {
    type Response = AVTransportResponse;

    fn action_name(&self) -> &'static str {
        match self {
            Self::SetAVTransportURI(_) => "SetAVTransportURI",
            Self::Play(_) => "Play",
            Self::Pause(_) => "Pause",
            Self::Stop(_) => "Stop",
            Self::GetTransportInfo(_) => "GetTransportInfo",
            Self::GetMediaInfo(_) => "GetMediaInfo",
            Self::GetPositionInfo(_) => "GetPositionInfo",
        }
    }

    fn namespace(&self) -> &'static str {
        "urn:schemas-upnp-org:service:AVTransport:1"
    }

    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError> {
        match self {
            Self::SetAVTransportURI(req) => {
                renderer
                    .av_transport
                    .set_transport_uri(req.current_uri.clone(), req.current_uri_metadata.clone())
                    .await?;
                Ok(AVTransportResponse::Empty(EmptyResponse {}))
            }
            Self::Play(req) => {
                renderer.av_transport.play().await?;
                Ok(AVTransportResponse::Empty(EmptyResponse {}))
            }
            Self::Pause(req) => {
                renderer.av_transport.pause().await?;
                Ok(AVTransportResponse::Empty(EmptyResponse {}))
            }
            Self::Stop(req) => {
                renderer.av_transport.stop().await?;
                Ok(AVTransportResponse::Empty(EmptyResponse {}))
            }
            Self::GetTransportInfo(req) => {
                let state = renderer.av_transport.get_state().await?;
                Ok(AVTransportResponse::TransportInfo(
                    GetTransportInfoResponse {
                        current_transport_state: "Playing".to_string(),
                        current_transport_status: "OK".to_string(),
                        current_speed: "1".to_string(),
                    },
                ))
            }
            _ => Err(SoapError::Soap(SoapProtocolError::InvalidAction(
                "Unknown AV Transport Action".to_string(),
            ))),
        }
    }
}
pub struct AVTransportService;
#[async_trait::async_trait]
impl UPnPService for AVTransportService {
    type Action = AVTransportAction;

    fn service_type() -> &'static str {
        "urn:schemas-upnp-org:service:AVTransport:1"
    }

    fn service_id() -> &'static str {
        "AVTransport"
    }

    fn service_name() -> &'static str {
        "AVTransport"
    }

    async fn handle_action(
        action: Self::Action,
        renderer: Arc<MediaRenderer>,
    ) -> Result<Response<Body>, SoapError> {
        debug!(action = ?action.action_name(), "Handling AVTransport action");

        let response = action.handle(renderer).await?;
        create_soap_response(Self::service_type(), response).map(IntoResponse::into_response)
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}
