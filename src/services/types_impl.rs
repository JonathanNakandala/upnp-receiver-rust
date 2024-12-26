use crate::services::types::ProtocolInfo;
use std::{fmt, time::Duration};

impl fmt::Display for ProtocolInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:*:{}:{}",
            self.protocol, self.mime_type, self.additional_info
        )
    }
}
