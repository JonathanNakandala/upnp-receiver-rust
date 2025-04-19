use crate::server::soap::types::soap_response::EmptyResponse;
use serde::Serialize;

use super::{TransportState, TransportStatus};
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetAVTransportURIResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetTransportInfoResponse {
    pub current_transport_state: TransportState,
    pub current_transport_status: TransportStatus,
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
#[derive(Serialize)]
#[serde(untagged)]
pub enum AVTransportResponse {
    Empty(EmptyResponse),
    SetAVTransportURI(SetAVTransportURIResponse),
    TransportInfo(GetTransportInfoResponse),
    MediaInfo(GetMediaInfoResponse),
    PositionInfo(GetPositionInfoResponse),
}
