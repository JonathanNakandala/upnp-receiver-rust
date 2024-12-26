use crate::server::soap::error::{SoapError, SoapProtocolError};
use serde::{Deserialize, Serialize};
use crate::server::soap::types::soap_response::EmptyResponse;

// Action Types
#[derive(Debug, Clone, Deserialize)]
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
impl TryFrom<&str> for AVTransportActionType {
    type Error = SoapError;

    fn try_from(action: &str) -> Result<Self, Self::Error> {
        let action = action.trim_matches('"');
        let action_name = action.split('#').last().unwrap_or(action);

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
    TransportInfo(GetTransportInfoResponse),
    MediaInfo(GetMediaInfoResponse),
    PositionInfo(GetPositionInfoResponse),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetAVTransportURIRequest {
    instance_id: u32,
    pub(crate) current_uri: String,
    pub(crate) current_uri_metadata: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayRequest {
    instance_id: u32,
    speed: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PauseRequest {
    instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StopRequest {
    instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetTransportInfoRequest {
    instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMediaInfoRequest {
    instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetPositionInfoRequest {
    instance_id: u32,
}

// Response Types
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
