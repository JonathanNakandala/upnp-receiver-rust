use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ConnectionManagerAction {
    GetProtocolInfo(GetProtocolInfoRequest),
    GetCurrentConnectionIDs(GetCurrentConnectionIDsRequest),
    GetCurrentConnectionInfo(GetCurrentConnectionInfoRequest),
}

#[derive(Debug, Deserialize)]
pub struct GetProtocolInfoRequest {}

#[derive(Debug, Deserialize)]
pub struct GetCurrentConnectionIDsRequest {}

#[derive(Debug, Deserialize)]
pub struct GetCurrentConnectionInfoRequest {
    #[serde(rename = "ConnectionID")]
    pub connection_id: i32,
}

#[derive(Debug, Serialize)]
pub struct GetProtocolInfoResponse {
    #[serde(rename = "Source")]
    pub source: String,
    #[serde(rename = "Sink")]
    pub sink: String,
}

#[derive(Debug, Serialize)]
pub struct GetCurrentConnectionIDsResponse {
    #[serde(rename = "ConnectionIDs")]
    pub connection_ids: String,
}

#[derive(Debug, Serialize)]
pub struct GetCurrentConnectionInfoResponse {
    #[serde(rename = "RcsID")]
    pub rcs_id: i32,
    #[serde(rename = "AVTransportID")]
    pub av_transport_id: i32,
    #[serde(rename = "ProtocolInfo")]
    pub protocol_info: String,
    #[serde(rename = "PeerConnectionManager")]
    pub peer_connection_manager: String,
    #[serde(rename = "PeerConnectionID")]
    pub peer_connection_id: i32,
    #[serde(rename = "Direction")]
    pub direction: String,
    #[serde(rename = "Status")]
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ConnectionManagerResponse {
    ProtocolInfo(GetProtocolInfoResponse),
    ConnectionIDs(GetCurrentConnectionIDsResponse),
    ConnectionInfo(GetCurrentConnectionInfoResponse),
}
