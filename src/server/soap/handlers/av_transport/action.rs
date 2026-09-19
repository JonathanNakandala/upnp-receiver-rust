use crate::server::eventing::{av_transport_last_change, AVTransportEventState, EventManager};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::av_transport::response::GetPositionInfoResponse;
use crate::server::soap::types::av_transport::{
    AVTransportResponse, GetMediaInfoResponse, GetTransportInfoResponse, PauseResponse,
    PlayResponse, SeekResponse, SetAVTransportURIResponse, SetNextAVTransportURIResponse,
    StopResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::MediaRenderer;
use std::sync::Arc;
use tracing::info;

use crate::server::soap::types::av_transport::AVTransportAction;

use super::utils::{
    format_duration, format_play_speed, parse_duration, parse_play_speed, SUPPORTED_PLAY_SPEEDS,
};

/// Publishes a `LastChange` NOTIFY reflecting the transport state right
/// after a state-changing action succeeds, so subscribed control points
/// find out immediately instead of waiting for their next GetTransportInfo
/// poll.
async fn publish_transport_state(renderer: &MediaRenderer, events: &Arc<EventManager>) {
    let event = AVTransportEventState::snapshot(renderer.av_transport.as_ref()).await;
    events.broadcast(av_transport_last_change(&event));
}

impl UPnPAction for AVTransportAction {
    type Response = AVTransportResponse;

    fn action_name(&self) -> &'static str {
        match self {
            Self::SetAVTransportURI(_) => "SetAVTransportURI",
            Self::SetNextAVTransportURI(_) => "SetNextAVTransportURI",
            Self::Play(_) => "Play",
            Self::Pause(_) => "Pause",
            Self::Stop(_) => "Stop",
            Self::Seek(_) => "Seek",
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
                info!(instance_id = req.instance_id, uri = %req.current_uri, "Handling SetAVTransportURI");
                renderer
                    .av_transport
                    .set_transport_uri(req.current_uri.clone(), req.current_uri_metadata.clone())
                    .await?;
                publish_transport_state(&renderer, &renderer.av_transport_events).await;
                Ok(AVTransportResponse::SetAVTransportURI(
                    SetAVTransportURIResponse {},
                ))
            }
            Self::SetNextAVTransportURI(req) => {
                info!(
                    instance_id = req.instance_id,
                    uri = %req.next_uri,
                    "Handling SetNextAVTransportURI"
                );
                renderer
                    .av_transport
                    .set_next_transport_uri(req.next_uri.clone(), req.next_uri_metadata.clone())
                    .await?;
                publish_transport_state(&renderer, &renderer.av_transport_events).await;
                Ok(AVTransportResponse::SetNextAVTransportURI(
                    SetNextAVTransportURIResponse {},
                ))
            }
            Self::Play(req) => {
                info!(instance_id = req.instance_id, speed = %req.speed, "Handling Play");

                let speed = parse_play_speed(&req.speed).ok_or_else(|| {
                    SoapError::Soap(SoapProtocolError::InvalidRequest(format!(
                        "Invalid play speed: {}",
                        req.speed
                    )))
                })?;
                if !SUPPORTED_PLAY_SPEEDS.contains(&speed) {
                    return Err(SoapError::Soap(SoapProtocolError::InvalidRequest(format!(
                        "Unsupported play speed: {} (supported: 1/4, 1/2, 1, 2, 4)",
                        req.speed
                    ))));
                }

                // play() alone is a no-op if we're already Playing (e.g. a
                // control point scrubbing speed mid-playback), so the speed
                // change still needs to happen unconditionally after it.
                renderer.av_transport.play().await?;
                renderer.av_transport.set_play_speed(speed).await?;
                publish_transport_state(&renderer, &renderer.av_transport_events).await;
                Ok(AVTransportResponse::Play(PlayResponse {}))
            }
            Self::Pause(req) => {
                info!(instance_id = req.instance_id, "Handling Pause");
                renderer.av_transport.pause().await?;
                publish_transport_state(&renderer, &renderer.av_transport_events).await;
                Ok(AVTransportResponse::Pause(PauseResponse {}))
            }
            Self::Stop(req) => {
                info!(instance_id = req.instance_id, "Handling Stop");
                renderer.av_transport.stop().await?;
                publish_transport_state(&renderer, &renderer.av_transport_events).await;
                Ok(AVTransportResponse::Stop(StopResponse {}))
            }
            Self::Seek(req) => {
                info!(instance_id = req.instance_id, unit = %req.unit, target = %req.target, "Handling Seek");
                if req.unit != "REL_TIME" && req.unit != "ABS_TIME" {
                    return Err(SoapError::Soap(SoapProtocolError::InvalidRequest(format!(
                        "Unsupported seek unit: {}",
                        req.unit
                    ))));
                }

                let target = parse_duration(&req.target).ok_or_else(|| {
                    SoapError::Soap(SoapProtocolError::InvalidRequest(format!(
                        "Invalid seek target: {}",
                        req.target
                    )))
                })?;

                renderer.av_transport.seek(target).await?;
                Ok(AVTransportResponse::Seek(SeekResponse {}))
            }
            Self::GetTransportInfo(req) => {
                info!(instance_id = req.instance_id, "Handling GetTransportInfo");
                let state = renderer.av_transport.get_state().await?;
                let status = renderer.av_transport.get_status().await?;
                let speed = renderer.av_transport.get_play_speed().await?;
                Ok(AVTransportResponse::TransportInfo(
                    GetTransportInfoResponse {
                        current_transport_state: state,
                        current_transport_status: status,
                        current_speed: format_play_speed(speed),
                    },
                ))
            }

            Self::GetMediaInfo(req) => {
                info!(instance_id = req.instance_id, "Handling GetMediaInfo");
                if let Some(media) = renderer.av_transport.get_media_info().await? {
                    Ok(AVTransportResponse::MediaInfo(GetMediaInfoResponse {
                        nr_tracks: 1,
                        media_duration: media
                            .duration
                            .map(format_duration)
                            .unwrap_or("00:00:00".to_string()),
                        current_uri: media.uri,
                        current_uri_metadata: media.metadata.unwrap_or_default(),
                        next_uri: media.next_uri,
                        next_uri_metadata: media.next_metadata.unwrap_or_default(),
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
                info!(instance_id = req.instance_id, "Handling GetPositionInfo");
                if let Some(position) = renderer.av_transport.get_position_info().await? {
                    // AbsTime is position since the start of the overall
                    // AVTransport stream, RelTime since the start of the
                    // current track — distinct only when a playlist can
                    // offset one from the other. We don't support playlists
                    // (always exactly one track, position 0), so they're the
                    // same real backend position, not two separate clocks.
                    let elapsed = position
                        .position
                        .map(format_duration)
                        .unwrap_or_else(|| "00:00:00".to_string());

                    Ok(AVTransportResponse::PositionInfo(GetPositionInfoResponse {
                        track: 1,
                        track_duration: position
                            .duration
                            .map(format_duration)
                            .unwrap_or_else(|| "00:00:00".to_string()),
                        track_metadata: position.metadata.unwrap_or_default(),
                        track_uri: position.uri,
                        rel_time: elapsed.clone(),
                        abs_time: elapsed,
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
        }
    }
}
