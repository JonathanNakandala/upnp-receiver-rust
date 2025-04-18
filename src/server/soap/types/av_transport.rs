use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::av_transport_request::{
    GetMediaInfoRequest, GetPositionInfoRequest, GetTransportInfoRequest, PauseRequest,
    PlayRequest, SetAVTransportURIRequest, StopRequest,
};
use crate::server::soap::types::soap_response::EmptyResponse;
use serde::{Deserialize, Serialize};
use tracing::info;

// Action Types
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AVTransportActionType {
    SetAVTransportURI,
    Play,
    Pause,
    Stop,
    GetTransportInfo,
    GetMediaInfo,
    GetPositionInfo,
}
impl TryFrom<&String> for AVTransportActionType {
    type Error = SoapError;

    fn try_from(action: &String) -> Result<Self, Self::Error> {
        let action = action.trim_matches('"');
        let action_name = action.split('#').last().unwrap_or(action);
        info!(
            action_name = action_name,
            "Matching Action Name with match statement"
        );
        match action_name {
            "SetAVTransportURI" => Ok(Self::SetAVTransportURI),
            "Play" => Ok(Self::Play),
            "Pause" => Ok(Self::Pause),
            "Stop" => Ok(Self::Stop),
            "GetTransportInfo" => Ok(Self::GetTransportInfo),
            "GetMediaInfo" => Ok(Self::GetMediaInfo),
            "GetPositionInfo" => Ok(Self::GetPositionInfo),
            _ => Err(SoapError::Soap(SoapProtocolError::InvalidAction(format!(
                "Unsupported AVTransport action: {}",
                action
            )))),
        }
    }
}

// Request Types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AVTransportAction {
    SetAVTransportURI(SetAVTransportURIRequest),
    Play(PlayRequest),
    Pause(PauseRequest),
    Stop(StopRequest),
    GetTransportInfo(GetTransportInfoRequest),
    GetMediaInfo(GetMediaInfoRequest),
    GetPositionInfo(GetPositionInfoRequest),
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum AVTransportResponse {
    Empty(EmptyResponse),
    SetAVTransportURI(SetAVTransportURIResponse),
    TransportInfo(GetTransportInfoResponse),
    MediaInfo(GetMediaInfoResponse),
    PositionInfo(GetPositionInfoResponse),
}

// Response Types
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetAVTransportURIResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetTransportInfoResponse {
    pub current_transport_state: String,
    pub current_transport_status: String,
    pub current_speed: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMediaInfoResponse {
    pub nr_tracks: u32,
    pub media_duration: String,
    pub current_uri: String,
    pub current_uri_metadata: String,
    pub next_uri: String,
    pub next_uri_metadata: String,
    pub play_medium: String,
    pub record_medium: String,
    pub write_status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetPositionInfoResponse {
    pub track: u32,
    pub track_duration: String,
    pub track_metadata: String,
    pub track_uri: String,
    pub rel_time: String,
    pub abs_time: String,
    pub rel_count: i32,
    pub abs_count: i32,
}
