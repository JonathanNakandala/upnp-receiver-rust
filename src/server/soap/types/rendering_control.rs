use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RenderingControlAction {
    SetVolume(SetVolumeRequest),
    SetMute(SetMuteRequest),
    GetVolume(GetVolumeRequest),
    GetMute(GetMuteRequest),
    SetVolumeDB(SetVolumeDBRequest),
    GetVolumeDB(GetVolumeDBRequest),
    GetVolumeDBRange(GetVolumeDBRangeRequest),
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVolumeDBRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
    pub desired_volume: i16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeDBRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeDBRangeRequest {
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
#[serde(rename_all = "PascalCase")]
pub struct SetVolumeResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetMuteResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeDBResponse {
    pub current_volume: i16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVolumeDBResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeDBRangeResponse {
    pub min_value: i16,
    pub max_value: i16,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RenderingControlResponse {
    SetVolume(SetVolumeResponse),
    SetMute(SetMuteResponse),
    Volume(GetVolumeResponse),
    Mute(GetMuteResponse),
    VolumeDB(GetVolumeDBResponse),
    SetVolumeDB(SetVolumeDBResponse),
    VolumeDBRange(GetVolumeDBRangeResponse),
}
