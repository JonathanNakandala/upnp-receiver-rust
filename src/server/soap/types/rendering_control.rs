use crate::server::soap::types::soap_response::EmptyResponse;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RenderingControlAction {
    SetVolume(SetVolumeRequest),
    SetMute(SetMuteRequest),
    GetVolume(GetVolumeRequest),
    GetMute(GetMuteRequest),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVolumeRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
    pub desired_volume: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetMuteRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
    pub desired_mute: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMuteRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
}

// Add response types
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeResponse {
    pub current_volume: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMuteResponse {
    pub current_mute: bool,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RenderingControlResponse {
    Empty(EmptyResponse),
    Volume(GetVolumeResponse),
    Mute(GetMuteResponse),
}
