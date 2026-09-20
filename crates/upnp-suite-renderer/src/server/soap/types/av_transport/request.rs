use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PlayRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    #[serde(rename = "Speed")]
    pub speed: String,
}

#[derive(Debug, Deserialize)]
pub struct PauseRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct StopRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetTransportInfoRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetMediaInfoRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetPositionInfoRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetDeviceCapabilitiesRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct SeekRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,

    #[serde(rename = "Unit")]
    pub unit: String,

    #[serde(rename = "Target")]
    pub target: String,
}

#[derive(Debug, Deserialize)]
pub struct SetAVTransportURIRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,

    #[serde(rename = "CurrentURI")]
    pub current_uri: String,

    #[serde(rename = "CurrentURIMetaData")]
    pub current_uri_metadata: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetTransportSettingsRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetCurrentTransportActionsRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct SetPlayModeRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,

    #[serde(rename = "NewPlayMode")]
    pub new_play_mode: String,
}

#[derive(Debug, Deserialize)]
pub struct NextRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct PreviousRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct SetNextAVTransportURIRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,

    #[serde(rename = "NextURI")]
    pub next_uri: String,

    #[serde(rename = "NextURIMetaData")]
    pub next_uri_metadata: Option<String>,
}
