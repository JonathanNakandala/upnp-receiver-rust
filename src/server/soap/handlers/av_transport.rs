use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::av_transport::{
    AVTransportResponse, GetMediaInfoResponse, GetPositionInfoResponse, GetTransportInfoResponse,
    SetAVTransportURIResponse,
};
use crate::server::soap::types::soap_response::EmptyResponse;
use crate::server::soap::types::{UPnPAction, UPnPService};
use crate::server::soap::utils::create_soap_response;
use crate::services::MediaRenderer;
use axum::body::Body;
use std::sync::Arc;

use crate::server::soap::types::av_transport_request::{
    GetMediaInfoRequest, GetPositionInfoRequest, GetTransportInfoRequest, PauseRequest,
    PlayRequest, SetAVTransportURIRequest, StopRequest, UPnPInstanceRequest,
};
use axum::http::Response;
use axum::response::IntoResponse;
use serde::de::VariantAccess;
use serde::Deserialize;
use tracing::debug;

#[derive(Debug, Deserialize)]
pub enum AVTransportAction {
    #[serde(rename = "u:SetAVTransportURI")]
    SetAVTransportURI(SetAVTransportURIRequest),
    #[serde(rename = "Play")]
    Play(PlayRequest),
    #[serde(rename = "Pause")]
    Pause(PauseRequest),
    #[serde(rename = "Stop")]
    Stop(StopRequest),
    #[serde(rename = "GetTransportInfo")]
    GetTransportInfo(GetTransportInfoRequest),
    #[serde(rename = "GetMediaInfo")]
    GetMediaInfo(GetMediaInfoRequest),
    #[serde(rename = "GetPositionInfo")]
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
                Ok(AVTransportResponse::SetAVTransportURI(
                    SetAVTransportURIResponse {},
                ))
            }
            Self::Play(req) => {
                renderer.av_transport.play().await?;
                Ok(AVTransportResponse::Empty(EmptyResponse {
                    action_name: "Play".to_string(),
                }))
            }
            Self::Pause(req) => {
                renderer.av_transport.pause().await?;
                Ok(AVTransportResponse::Empty(EmptyResponse {
                    action_name: "Pause".to_string(),
                }))
            }
            Self::Stop(req) => {
                renderer.av_transport.stop().await?;
                Ok(AVTransportResponse::Empty(EmptyResponse {
                    action_name: "Stop".to_string(),
                }))
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

            Self::GetMediaInfo(req) => {
                if let Some(media) = renderer.av_transport.get_media_info().await? {
                    Ok(AVTransportResponse::MediaInfo(GetMediaInfoResponse {
                        nr_tracks: 1,
                        media_duration: media
                            .duration
                            .map(format_duration)
                            .unwrap_or("00:00:00".to_string()),
                        current_uri: media.uri,
                        current_uri_metadata: media.metadata.unwrap_or_default(),
                        next_uri: "".to_string(),
                        next_uri_metadata: "".to_string(),
                        play_medium: "NETWORK".to_string(),
                        record_medium: "NOT_IMPLEMENTED".to_string(),
                        write_status: "NOT_IMPLEMENTED".to_string(),
                    }))
                } else {
                    Ok(AVTransportResponse::MediaInfo(GetMediaInfoResponse {
                        nr_tracks: 0,
                        media_duration: "00:00:00".to_string(),
                        current_uri: "".to_string(),
                        current_uri_metadata: "".to_string(),
                        next_uri: "".to_string(),
                        next_uri_metadata: "".to_string(),
                        play_medium: "NONE".to_string(),
                        record_medium: "NOT_IMPLEMENTED".to_string(),
                        write_status: "NOT_IMPLEMENTED".to_string(),
                    }))
                }
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

    fn parse_action(
        action_name: &str,
        content: &serde_json::Value,
    ) -> Result<Self::Action, String> {
        match action_name {
            "SetAVTransportURI" => {
                let instance_id = content
                    .get("InstanceID")
                    .and_then(|v| v.get("$text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("0")
                    .to_string();

                let current_uri = content
                    .get("CurrentURI")
                    .and_then(|v| v.get("$text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let current_uri_metadata = content
                    .get("CurrentURIMetaData")
                    .and_then(|v| v.get("$text"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // Create the request struct
                let request = SetAVTransportURIRequest {
                    instance_id,
                    current_uri,
                    current_uri_metadata,
                };

                Ok(AVTransportAction::SetAVTransportURI(request))
            }
            "GetMediaInfo" => Ok(AVTransportAction::GetMediaInfo(GetMediaInfoRequest {
                base: UPnPInstanceRequest { instance_id: 1 },
            })),
            _ => Err(format!("Unknown AVTransport action: {}", action_name)),
        }
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}
