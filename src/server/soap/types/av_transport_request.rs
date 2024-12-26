use serde::Deserialize;
use quick_xml::de::{from_str, DeError};
use serde::de::{Deserializer, Visitor};
use std::fmt;

pub trait UPnPRequestWithInstance {
    fn instance_id(&self) -> u32;
}

// Base struct for all requests with an InstanceID
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

// Struct for SetAVTransportURIRequest
#[derive(Debug, Deserialize)]
pub struct SetAVTransportURIRequest {
    #[serde(rename = "InstanceID", deserialize_with = "deserialize_text")]
    pub instance_id: String,

    #[serde(rename = "CurrentURI", deserialize_with = "deserialize_text")]
    pub current_uri: String,

    #[serde(rename = "CurrentURIMetaData", deserialize_with = "deserialize_optional_text")]
    pub current_uri_metadata: Option<String>,
}

// Define the TextVisitor once so it can be reused
struct TextVisitor;

impl<'de> Visitor<'de> for TextVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value.to_string())
    }
}

// Custom deserializer for elements that contain text
fn deserialize_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(TextVisitor)
}

// Custom deserializer for optional elements
fn deserialize_optional_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionalTextVisitor;

    impl<'de> Visitor<'de> for OptionalTextVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an optional string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_str(TextVisitor).map(Some)
        }
    }

    deserializer.deserialize_option(OptionalTextVisitor)
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

#[derive(Deserialize)]
#[serde(untagged)]
enum OptionalTextWrapper {
    None,
    Text(String),
    Object { #[serde(rename = "$text")] text: String },
    EmptyObject {},
}

