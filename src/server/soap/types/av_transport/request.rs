use serde::de::{Deserializer, Visitor};
use serde::Deserialize;
use std::fmt;

pub trait UPnPRequestWithInstance {
    fn instance_id(&self) -> u32;
}
#[derive(Debug, Deserialize)]
pub struct UPnPInstanceRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

impl UPnPRequestWithInstance for UPnPInstanceRequest {
    fn instance_id(&self) -> u32 {
        self.instance_id
    }
}
// Struct for PlayRequest
#[derive(Debug, Deserialize)]
pub struct PlayRequest {
    #[serde(flatten)]
    pub base: UPnPInstanceRequest,
    #[serde(rename = "Speed")]
    pub speed: String,
}

// Struct for PauseRequest
#[derive(Debug, Deserialize)]
pub struct PauseRequest {
    #[serde(flatten)]
    pub base: UPnPInstanceRequest,
}

// Struct for StopRequest
#[derive(Debug, Deserialize)]
pub struct StopRequest {
    #[serde(flatten)]
    pub base: UPnPInstanceRequest,
}

// Struct for GetTransportInfoRequest
#[derive(Debug, Deserialize)]
pub struct GetTransportInfoRequest {
    #[serde(flatten)]
    pub base: UPnPInstanceRequest,
}

// Struct for GetMediaInfoRequest
#[derive(Debug, Deserialize)]
pub struct GetMediaInfoRequest {
    #[serde(flatten)]
    pub base: UPnPInstanceRequest,
}

// Struct for GetPositionInfoRequest
#[derive(Debug, Deserialize)]
pub struct GetPositionInfoRequest {
    #[serde(flatten)]
    pub base: UPnPInstanceRequest,
}

#[derive(Debug, Deserialize)]
pub struct SetAVTransportURIRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: String,

    #[serde(rename = "CurrentURI")]
    pub current_uri: String,

    #[serde(rename = "CurrentURIMetaData")]
    pub current_uri_metadata: Option<String>,
}
