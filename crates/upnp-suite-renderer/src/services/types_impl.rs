use crate::services::types::{ConnectionDirection, ConnectionManagerStatus, ProtocolInfo};
use std::fmt;

impl fmt::Display for ProtocolInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}",
            self.protocol, self.network, self.mime_type, self.additional_info
        )
    }
}

impl fmt::Display for ConnectionDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Input => "Input",
            Self::Output => "Output",
        })
    }
}

impl fmt::Display for ConnectionManagerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "OK",
            Self::ContentFormatMismatch => "ContentFormatMismatch",
            Self::InsufficientBandwidth => "InsufficientBandwidth",
            Self::UnreliableChannel => "UnreliableChannel",
            Self::Unknown => "Unknown",
        })
    }
}
