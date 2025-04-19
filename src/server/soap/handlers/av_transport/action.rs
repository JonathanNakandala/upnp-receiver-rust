use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::av_transport::response::GetPositionInfoResponse;
use crate::server::soap::types::av_transport::{
    AVTransportResponse, GetMediaInfoResponse, GetTransportInfoResponse, SetAVTransportURIResponse,
    TransportState, TransportStatus,
};
use crate::server::soap::types::soap_response::EmptyResponse;
use crate::server::soap::types::UPnPAction;
use crate::services::MediaRenderer;
use std::sync::Arc;

use crate::server::soap::types::av_transport::AVTransportAction;

use super::utils::format_duration;

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
                        current_transport_state: TransportState::PLAYING,
                        current_transport_status: TransportStatus::OK,
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

            Self::GetPositionInfo(req) => {
                if let Some(position) = renderer.av_transport.get_position_info().await? {
                    Ok(AVTransportResponse::PositionInfo(GetPositionInfoResponse {
                        track: 1,
                        track_duration: position
                            .duration
                            .map(format_duration)
                            .unwrap_or_else(|| "00:00:00".to_string()),
                        track_metadata: position.metadata.unwrap_or_default(),
                        track_uri: position.uri,
                        rel_time: position
                            .position
                            .map(format_duration)
                            .unwrap_or_else(|| "00:00:00".to_string()),
                        abs_time: "00:00:00".to_string(),
                        rel_count: 0,
                        abs_count: 0,
                    }))
                } else {
                    Ok(AVTransportResponse::PositionInfo(GetPositionInfoResponse {
                        track: 0,
                        track_duration: "00:00:00".to_string(),
                        track_metadata: "".to_string(),
                        track_uri: "".to_string(),
                        rel_time: "00:00:00".to_string(),
                        abs_time: "00:00:00".to_string(),
                        rel_count: 0,
                        abs_count: 0,
                    }))
                }
            }

            _ => Err(SoapError::Soap(SoapProtocolError::InvalidAction(
                "Unknown AV Transport Action".to_string(),
            ))),
        }
    }
}
