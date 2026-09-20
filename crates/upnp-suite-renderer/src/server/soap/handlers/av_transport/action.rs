use crate::server::eventing::{
    av_transport_last_change, transport_actions_for, AVTransportEventState, EventManager,
};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::av_transport::response::GetPositionInfoResponse;
use crate::server::soap::types::av_transport::{
    AVTransportResponse, GetCurrentTransportActionsResponse, GetDeviceCapabilitiesResponse,
    GetMediaInfoResponse, GetTransportInfoResponse, GetTransportSettingsResponse, NextResponse,
    PauseResponse, PlayMode, PlayResponse, PreviousResponse, SeekResponse,
    SetAVTransportURIResponse, SetNextAVTransportURIResponse, SetPlayModeResponse, StopResponse,
    TransportState,
};
use crate::server::soap::types::UPnPAction;
use crate::services::av_transport::AVTransport;
use crate::services::instance::InstanceId;
use crate::services::MediaRenderer;
use std::sync::Arc;
use tracing::{info, warn};

use crate::server::soap::types::av_transport::AVTransportAction;

use super::utils::{
    format_duration, format_play_speed, parse_duration, parse_play_speed, SUPPORTED_PLAY_SPEEDS,
};

/// Publishes a `LastChange` NOTIFY reflecting the transport state right
/// after a state-changing action succeeds, so subscribed control points
/// find out immediately instead of waiting for their next GetTransportInfo
/// poll. `instance_id`/`transport` are the instance the triggering action
/// was actually resolved against — see `AVTransportAction::handle` below.
async fn publish_transport_state(
    instance_id: InstanceId,
    transport: &dyn AVTransport,
    events: &Arc<EventManager>,
) {
    let event = AVTransportEventState::snapshot(instance_id, transport).await;
    events.broadcast(av_transport_last_change(&event));
}

/// Publishes a synthetic `LastChange` NOTIFY announcing `TRANSITIONING`
/// right before a state-changing action is attempted. Per the AVTransport
/// spec, `Play`/`Pause`/`Stop`/`SetAVTransportURI` "may temporarily go to
/// the TRANSITIONING state" and `Seek` "changes TransportState to
/// TRANSITIONING and then returns immediately" — real transitions here are
/// effectively instant (no buffering/seek-completion wait), so this isn't a
/// real interim backend state, just an event a subscriber relying on
/// eventing (rather than polling) can observe before the real post-action
/// state follows right behind it.
async fn publish_transitioning(
    instance_id: InstanceId,
    transport: &dyn AVTransport,
    events: &Arc<EventManager>,
) {
    let mut event = AVTransportEventState::snapshot(instance_id, transport).await;
    event.transport_state = TransportState::Transitioning;
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
            Self::Next(_) => "Next",
            Self::Previous(_) => "Previous",
            Self::GetTransportInfo(_) => "GetTransportInfo",
            Self::GetMediaInfo(_) => "GetMediaInfo",
            Self::GetPositionInfo(_) => "GetPositionInfo",
            Self::SetPlayMode(_) => "SetPlayMode",
            Self::GetTransportSettings(_) => "GetTransportSettings",
            Self::GetCurrentTransportActions(_) => "GetCurrentTransportActions",
            Self::GetDeviceCapabilities(_) => "GetDeviceCapabilities",
        }
    }

    fn namespace(&self) -> &'static str {
        "urn:schemas-upnp-org:service:AVTransport:1"
    }

    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError> {
        // Resolved once per request: an InstanceID nothing was created under
        // (i.e. anything but 0 today — see `crate::services::instance`)
        // fails here with UPnP error 718, before any action-specific logic
        // runs, rather than silently acting on the wrong (or a default)
        // instance.
        let instance_id = self.instance_id();
        let transport = renderer.av_transport.get(instance_id)?;

        match self {
            Self::SetAVTransportURI(req) => {
                publish_transitioning(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                let result = transport
                    .set_transport_uri(req.current_uri.clone(), req.current_uri_metadata.clone())
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        uri = %req.current_uri,
                        "Handling SetAVTransportURI"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        uri = %req.current_uri,
                        error = %err,
                        "Handling SetAVTransportURI"
                    ),
                }
                // Published whether this succeeded or not: a failed action
                // still needs its subscribers corrected off the
                // `publish_transitioning` above, or they're left believing
                // the transport is still transitioning.
                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                result?;
                Ok(AVTransportResponse::SetAVTransportURI(
                    SetAVTransportURIResponse {},
                ))
            }
            Self::SetNextAVTransportURI(req) => {
                let result = transport
                    .set_next_transport_uri(req.next_uri.clone(), req.next_uri_metadata.clone())
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        uri = %req.next_uri,
                        "Handling SetNextAVTransportURI"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        uri = %req.next_uri,
                        error = %err,
                        "Handling SetNextAVTransportURI"
                    ),
                }
                result?;
                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                Ok(AVTransportResponse::SetNextAVTransportURI(
                    SetNextAVTransportURIResponse {},
                ))
            }
            Self::Play(req) => {
                publish_transitioning(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                let result = async {
                    let speed = parse_play_speed(&req.speed).ok_or_else(|| {
                        SoapError::Soap(SoapProtocolError::PlaySpeedNotSupported(format!(
                            "Invalid play speed: {}",
                            req.speed
                        )))
                    })?;
                    if !SUPPORTED_PLAY_SPEEDS.contains(&speed) {
                        return Err(SoapError::Soap(SoapProtocolError::PlaySpeedNotSupported(
                            format!(
                                "Unsupported play speed: {} (supported: 1/4, 1/2, 1, 2, 4)",
                                req.speed
                            ),
                        )));
                    }

                    // play() alone is a no-op if we're already Playing (e.g. a
                    // control point scrubbing speed mid-playback), so the speed
                    // change still needs to happen unconditionally after it.
                    transport.play().await?;
                    transport.set_play_speed(speed).await?;
                    Ok(())
                }
                .await;

                match &result {
                    Ok(_) => info!(instance_id = req.instance_id, speed = %req.speed, "Handling Play"),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        speed = %req.speed,
                        error = %err,
                        "Handling Play"
                    ),
                }

                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                result?;
                Ok(AVTransportResponse::Play(PlayResponse {}))
            }
            Self::Pause(req) => {
                publish_transitioning(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                let result = transport.pause().await;
                match &result {
                    Ok(_) => info!(instance_id = req.instance_id, "Handling Pause"),
                    Err(err) => warn!(instance_id = req.instance_id, error = %err, "Handling Pause"),
                }
                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                result?;
                Ok(AVTransportResponse::Pause(PauseResponse {}))
            }
            Self::Stop(req) => {
                publish_transitioning(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                let result = transport.stop().await;
                match &result {
                    Ok(_) => info!(instance_id = req.instance_id, "Handling Stop"),
                    Err(err) => warn!(instance_id = req.instance_id, error = %err, "Handling Stop"),
                }
                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                result?;
                Ok(AVTransportResponse::Stop(StopResponse {}))
            }
            Self::Seek(req) => {
                publish_transitioning(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                let result = async {
                    if req.unit != "REL_TIME" && req.unit != "ABS_TIME" {
                        return Err(SoapError::Soap(SoapProtocolError::SeekModeNotSupported(
                            format!("Unsupported seek unit: {}", req.unit),
                        )));
                    }

                    let target = parse_duration(&req.target).ok_or_else(|| {
                        SoapError::Soap(SoapProtocolError::IllegalSeekTarget(format!(
                            "Invalid seek target: {}",
                            req.target
                        )))
                    })?;

                    transport.seek(target).await?;
                    Ok(())
                }
                .await;

                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        unit = %req.unit,
                        target = %req.target,
                        "Handling Seek"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        unit = %req.unit,
                        target = %req.target,
                        error = %err,
                        "Handling Seek"
                    ),
                }

                // Seek doesn't change TransportState itself, but the
                // TRANSITIONING announced above still needs a follow-up
                // event reverting subscribers back to the real state, per
                // the spec's "when the desired position is reached,
                // TransportState will return to the previous transport
                // state" — that "return" is only observable through this.
                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                result?;
                Ok(AVTransportResponse::Seek(SeekResponse {}))
            }
            Self::Next(req) => {
                let result = transport.next().await;
                match &result {
                    Ok(_) => info!(instance_id = req.instance_id, "Handling Next"),
                    Err(err) => warn!(instance_id = req.instance_id, error = %err, "Handling Next"),
                }
                result?;
                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                Ok(AVTransportResponse::Next(NextResponse {}))
            }
            Self::Previous(req) => {
                let result = transport.previous().await;
                match &result {
                    Ok(_) => info!(instance_id = req.instance_id, "Handling Previous"),
                    Err(err) => {
                        warn!(instance_id = req.instance_id, error = %err, "Handling Previous")
                    }
                }
                result?;
                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                Ok(AVTransportResponse::Previous(PreviousResponse {}))
            }
            Self::GetTransportInfo(req) => {
                let result: Result<_, SoapError> = async {
                    let state = transport.get_state().await?;
                    let status = transport.get_status().await?;
                    let speed = transport.get_play_speed().await?;
                    Ok((state, status, speed))
                }
                .await;

                match &result {
                    Ok((state, status, speed)) => info!(
                        instance_id = req.instance_id,
                        current_transport_state = ?state,
                        current_transport_status = ?status,
                        current_speed = %format_play_speed(*speed),
                        "Handling GetTransportInfo"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetTransportInfo"
                    ),
                }

                let (state, status, speed) = result?;
                Ok(AVTransportResponse::TransportInfo(
                    GetTransportInfoResponse {
                        current_transport_state: state,
                        current_transport_status: status,
                        current_speed: format_play_speed(speed),
                    },
                ))
            }

            Self::GetMediaInfo(req) => {
                let result = transport.get_media_info().await;
                match &result {
                    Ok(Some(media)) => info!(
                        instance_id = req.instance_id,
                        current_uri = %media.uri,
                        "Handling GetMediaInfo"
                    ),
                    Ok(None) => info!(
                        instance_id = req.instance_id,
                        current_uri = "",
                        "Handling GetMediaInfo"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetMediaInfo"
                    ),
                }

                if let Some(media) = result? {
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
                let result = transport.get_position_info().await;
                match &result {
                    Ok(Some(position)) => info!(
                        instance_id = req.instance_id,
                        track_uri = %position.uri,
                        position = ?position.position,
                        "Handling GetPositionInfo"
                    ),
                    Ok(None) => info!(
                        instance_id = req.instance_id,
                        track_uri = "",
                        "Handling GetPositionInfo"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetPositionInfo"
                    ),
                }

                if let Some(position) = result? {
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

            Self::SetPlayMode(req) => {
                let result: Result<(), SoapError> = async {
                    let mode = PlayMode::parse(&req.new_play_mode).ok_or_else(|| {
                        SoapError::Soap(SoapProtocolError::PlayModeNotSupported(format!(
                            "Unsupported play mode: {}",
                            req.new_play_mode
                        )))
                    })?;
                    transport.set_play_mode(mode).await?;
                    Ok(())
                }
                .await;

                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        play_mode = %req.new_play_mode,
                        "Handling SetPlayMode"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        play_mode = %req.new_play_mode,
                        error = %err,
                        "Handling SetPlayMode"
                    ),
                }

                result?;
                publish_transport_state(instance_id, transport.as_ref(), &renderer.av_transport_events)
                    .await;
                Ok(AVTransportResponse::SetPlayMode(SetPlayModeResponse {}))
            }

            Self::GetTransportSettings(req) => {
                let result = transport.get_play_mode().await;
                match &result {
                    Ok(mode) => info!(
                        instance_id = req.instance_id,
                        play_mode = mode.as_str(),
                        "Handling GetTransportSettings"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetTransportSettings"
                    ),
                }

                let mode = result?;
                Ok(AVTransportResponse::TransportSettings(
                    GetTransportSettingsResponse {
                        play_mode: mode.as_str().to_string(),
                        rec_quality_mode: "NOT_IMPLEMENTED".to_string(),
                    },
                ))
            }

            Self::GetCurrentTransportActions(req) => {
                let result: Result<_, SoapError> = async {
                    let state = transport.get_state().await?;
                    let media = transport.get_media_info().await?;
                    Ok((state, media))
                }
                .await;

                match &result {
                    Ok((state, _)) => info!(
                        instance_id = req.instance_id,
                        current_transport_state = ?state,
                        "Handling GetCurrentTransportActions"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetCurrentTransportActions"
                    ),
                }

                let (state, media) = result?;
                let has_next = media.map(|m| !m.next_uri.is_empty()).unwrap_or(false);
                Ok(AVTransportResponse::CurrentTransportActions(
                    GetCurrentTransportActionsResponse {
                        actions: transport_actions_for(state, has_next),
                    },
                ))
            }

            Self::GetDeviceCapabilities(req) => {
                // Static for this renderer: it only ever plays a network
                // stream (or nothing), and doesn't record at all — so unlike
                // GetTransportSettings/GetCurrentTransportActions, there's no
                // per-instance state to read here.
                info!(
                    instance_id = req.instance_id,
                    "Handling GetDeviceCapabilities"
                );
                Ok(AVTransportResponse::DeviceCapabilities(
                    GetDeviceCapabilitiesResponse {
                        play_media: "NETWORK,NONE".to_string(),
                        rec_media: "NOT_IMPLEMENTED".to_string(),
                        rec_quality_modes: "NOT_IMPLEMENTED".to_string(),
                    },
                ))
            }
        }
    }
}
