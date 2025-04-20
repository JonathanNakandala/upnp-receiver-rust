use serde::{Deserialize, Serialize};

use crate::server::soap::types::av_transport::TransportState;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaCommand {
    Ping,
    Shutdown,
    Play,
    Pause,
    Stop,
    GetPosition,
    GetDuration,
    GetState,
    Load {
        uri: String,
        metadata: Option<String>,
    },
    Seek {
        position: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MediaResponse {
    Ok,
    Pong,
    Shutdown,
    Error { message: String },
    Position { milliseconds: u64 },
    Duration { milliseconds: Option<u64> },
    State { state: TransportState },
}
