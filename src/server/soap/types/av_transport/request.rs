use serde::Deserialize;

// Struct for PlayRequest
#[derive(Debug, Deserialize)]
pub struct PlayRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    #[serde(rename = "Speed")]
    pub speed: String,
}

// Struct for PauseRequest
#[derive(Debug, Deserialize)]
pub struct PauseRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

// Struct for StopRequest
#[derive(Debug, Deserialize)]
pub struct StopRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

// Request Data for urn:schemas-upnp-org:service:AVTransport:1#GetTransportInfo
#[derive(Debug, Deserialize)]
pub struct GetTransportInfoRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

// Request Data for urn:schemas-upnp-org:service:AVTransport:1#GetMediaInfo
#[derive(Debug, Deserialize)]
pub struct GetMediaInfoRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

// Struct for GetPositionInfoRequest
#[derive(Debug, Deserialize)]
pub struct GetPositionInfoRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
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
