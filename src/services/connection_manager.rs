use super::error::ServiceError;
use super::types::{ConnectionDirection, ConnectionInfo, ConnectionManagerStatus, ConnectionState, ProtocolInfo};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages UPnP connection and protocol information
#[async_trait]
pub trait ConnectionManager: Send + Sync {
    /// Returns the current connection status
    async fn get_status(&self) -> Result<ConnectionState, ServiceError>;

    /// Returns supported protocol information in UPnP format
    async fn get_protocol_info(&self) -> Result<Vec<ProtocolInfo>, ServiceError>;

    /// Returns the IDs of all currently active connections (just one, always
    /// "0", for a renderer that only ever has a single logical stream).
    async fn get_current_connection_ids(&self) -> Result<Vec<i32>, ServiceError>;

    /// Returns details for a connection ID, or `None` if it doesn't exist
    /// (maps to UPnP error 706, InvalidConnectionReference).
    async fn get_current_connection_info(
        &self,
        connection_id: i32,
    ) -> Result<Option<ConnectionInfo>, ServiceError>;
}

pub struct DummyConnectionManager {
    state: Arc<RwLock<ConnectionState>>,
    protocol_info: Vec<ProtocolInfo>,
}

/// Maps a mime type to its registered DLNA media format profile (`DLNA.ORG_PN`),
/// where one exists — this is what lets strict DLNA control points (Sonos-style
/// apps, some Samsung/BubbleUPnP transcoding logic) recognize a format as
/// natively playable instead of assuming it needs transcoding. Formats with
/// no DLNA-registered profile (Ogg/Vorbis, FLAC) fall back to a plain `*`,
/// which is spec-legal but gives those control points no profile to key off.
fn dlna_profile_for_mime_type(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        "audio/mpeg" | "audio/x-mpeg" => Some("DLNA.ORG_PN=MP3"),
        "audio/mp4" | "audio/x-m4a" | "audio/aac" => Some("DLNA.ORG_PN=AAC_ISO"),
        "audio/vnd.dlna.adts" => Some("DLNA.ORG_PN=AAC_ADTS"),
        "audio/wav" | "audio/x-wav" | "audio/vnd.wave" | "audio/L16" => {
            Some("DLNA.ORG_PN=LPCM")
        }
        "audio/x-ms-wma" => Some("DLNA.ORG_PN=WMABASE"),
        _ => None,
    }
}

impl Default for DummyConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DummyConnectionManager {
    pub fn new() -> Self {
        Self::with_supported_mime_types(vec!["audio/mpeg".to_string()])
    }

    /// Builds the `protocolInfo` list `GetProtocolInfo` reports from a list
    /// of supported mime types, one `http-get:*:<mime>:<additional-info>`
    /// sink entry each — e.g. from [`gstreamer::supported_mime_types`](crate::backend::gstreamer::supported_mime_types)
    /// so the renderer only advertises formats it can actually decode. Where
    /// a mime type has a registered DLNA profile, `additional_info` carries
    /// `DLNA.ORG_PN=...` instead of a bare `*` (see `dlna_profile_for_mime_type`).
    pub fn with_supported_mime_types(mime_types: Vec<String>) -> Self {
        let protocol_info = mime_types
            .into_iter()
            .map(|mime_type| {
                let additional_info = dlna_profile_for_mime_type(&mime_type)
                    .unwrap_or("*")
                    .to_string();
                ProtocolInfo {
                    protocol: "http-get".to_string(),
                    network: "*".to_string(),
                    mime_type,
                    additional_info,
                }
            })
            .collect();

        Self {
            state: Arc::new(RwLock::new(ConnectionState::Connected)),
            protocol_info,
        }
    }
}

#[async_trait]
impl ConnectionManager for DummyConnectionManager {
    async fn get_status(&self) -> Result<ConnectionState, ServiceError> {
        Ok(*self.state.read().await)
    }

    async fn get_protocol_info(&self) -> Result<Vec<ProtocolInfo>, ServiceError> {
        Ok(self.protocol_info.clone())
    }

    async fn get_current_connection_ids(&self) -> Result<Vec<i32>, ServiceError> {
        // A renderer with no real backend, or a backend that only ever plays
        // one thing at a time, has exactly one logical connection.
        Ok(vec![0])
    }

    async fn get_current_connection_info(
        &self,
        connection_id: i32,
    ) -> Result<Option<ConnectionInfo>, ServiceError> {
        if connection_id != 0 {
            return Ok(None);
        }

        Ok(Some(ConnectionInfo {
            connection_id: 0,
            rcs_id: 0,
            av_transport_id: 0,
            protocol_info: String::new(),
            peer_connection_manager: String::new(),
            peer_connection_id: -1,
            direction: ConnectionDirection::Input,
            status: ConnectionManagerStatus::Ok,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_a_single_connection() {
        let manager = DummyConnectionManager::new();
        assert_eq!(manager.get_current_connection_ids().await.unwrap(), vec![0]);
    }

    #[tokio::test]
    async fn returns_connection_info_for_known_id() {
        let manager = DummyConnectionManager::new();
        let info = manager
            .get_current_connection_info(0)
            .await
            .unwrap()
            .expect("connection 0 should exist");
        assert_eq!(info.connection_id, 0);
        assert_eq!(info.direction, ConnectionDirection::Input);
        assert_eq!(info.status, ConnectionManagerStatus::Ok);
    }

    #[tokio::test]
    async fn advertises_dlna_profile_for_known_mime_types() {
        let manager = DummyConnectionManager::with_supported_mime_types(vec![
            "audio/mpeg".to_string(),
            "audio/ogg".to_string(),
        ]);
        let infos = manager.get_protocol_info().await.unwrap();
        assert_eq!(infos[0].additional_info, "DLNA.ORG_PN=MP3");
        assert_eq!(infos[1].additional_info, "*");
    }

    #[tokio::test]
    async fn returns_none_for_unknown_connection_id() {
        let manager = DummyConnectionManager::new();
        assert!(manager
            .get_current_connection_info(99)
            .await
            .unwrap()
            .is_none());
    }
}
