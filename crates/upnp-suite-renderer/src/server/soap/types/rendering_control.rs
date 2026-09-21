use crate::services::instance::InstanceId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RenderingControlAction {
    ListPresets(ListPresetsRequest),
    SelectPreset(SelectPresetRequest),
    SetVolume(SetVolumeRequest),
    SetMute(SetMuteRequest),
    GetVolume(GetVolumeRequest),
    GetMute(GetMuteRequest),
    SetVolumeDB(SetVolumeDBRequest),
    GetVolumeDB(GetVolumeDBRequest),
    GetVolumeDBRange(GetVolumeDBRangeRequest),
    SetBrightness(SetBrightnessRequest),
    GetBrightness(GetBrightnessRequest),
    SetContrast(SetContrastRequest),
    GetContrast(GetContrastRequest),
}

impl RenderingControlAction {
    /// The `InstanceID` the control point sent with this action — see
    /// `AVTransportAction::instance_id` for why this is infallible.
    pub fn instance_id(&self) -> InstanceId {
        let raw = match self {
            Self::ListPresets(r) => r.instance_id,
            Self::SelectPreset(r) => r.instance_id,
            Self::SetVolume(r) => r.instance_id,
            Self::SetMute(r) => r.instance_id,
            Self::GetVolume(r) => r.instance_id,
            Self::GetMute(r) => r.instance_id,
            Self::SetVolumeDB(r) => r.instance_id,
            Self::GetVolumeDB(r) => r.instance_id,
            Self::GetVolumeDBRange(r) => r.instance_id,
            Self::SetBrightness(r) => r.instance_id,
            Self::GetBrightness(r) => r.instance_id,
            Self::SetContrast(r) => r.instance_id,
            Self::GetContrast(r) => r.instance_id,
        };
        InstanceId::new(raw)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListPresetsRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SelectPresetRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub preset_name: String,
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

// Brightness/Contrast take no Channel argument (Table 32/34/36/38 in the
// RenderingControl spec) - unlike Volume/Mute/VolumeDB, they're a display
// setting, not per audio-channel.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetBrightnessRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_brightness: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBrightnessRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetContrastRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_contrast: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetContrastRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
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
pub struct ListPresetsResponse {
    pub current_preset_name_list: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SelectPresetResponse {}

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
#[serde(rename_all = "PascalCase")]
pub struct SetBrightnessResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBrightnessResponse {
    pub current_brightness: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetContrastResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetContrastResponse {
    pub current_contrast: u16,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RenderingControlResponse {
    ListPresets(ListPresetsResponse),
    SelectPreset(SelectPresetResponse),
    SetVolume(SetVolumeResponse),
    SetMute(SetMuteResponse),
    Volume(GetVolumeResponse),
    Mute(GetMuteResponse),
    VolumeDB(GetVolumeDBResponse),
    SetVolumeDB(SetVolumeDBResponse),
    VolumeDBRange(GetVolumeDBRangeResponse),
    SetBrightness(SetBrightnessResponse),
    Brightness(GetBrightnessResponse),
    SetContrast(SetContrastResponse),
    Contrast(GetContrastResponse),
}
