use super::error::{ConnectionError, ServiceError};
use super::types::{
    ConnectionDirection, ConnectionInfo, ConnectionManagerStatus, ConnectionState,
    PreparedConnection, ProtocolInfo,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// The `ConnectionManager:3` `FeatureList` state variable value for a device
/// that implements no optional CM features (no CLOCKSYNC support). Still a
/// well-formed, empty `Features` document — required by `GetFeatureList()`
/// regardless of whether there's anything to list.
const EMPTY_FEATURE_LIST: &str = concat!(
    r#"<?xml version="1.0" encoding="utf-8"?>"#,
    r#"<Features xmlns="urn:schemas-upnp-org:av:cm-featureList" "#,
    r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" "#,
    r#"xsi:schemaLocation="urn:schemas-upnp-org:av:cm-featureList "#,
    r#"http://www.upnp.org/schemas/av/cm-featureList.xsd"></Features>"#,
);

/// Manages UPnP connection and protocol information
#[async_trait]
pub trait ConnectionManager: Send + Sync {
    /// Returns the current connection status
    async fn get_status(&self) -> Result<ConnectionState, ServiceError>;

    /// Returns supported protocol information in UPnP format
    async fn get_protocol_info(&self) -> Result<Vec<ProtocolInfo>, ServiceError>;

    /// Returns the IDs of all currently active connections: always includes
    /// the implicit default connection `0` (which exists for the lifetime of
    /// the renderer, for control points that never call
    /// `PrepareForConnection`), plus any connections allocated by
    /// `prepare_connection`.
    async fn get_current_connection_ids(&self) -> Result<Vec<i32>, ServiceError>;

    /// Returns details for a connection ID, or `None` if it doesn't exist
    /// (maps to UPnP error 706, InvalidConnectionReference).
    async fn get_current_connection_info(
        &self,
        connection_id: i32,
    ) -> Result<Option<ConnectionInfo>, ServiceError>;

    /// UPnP's `PrepareForConnection()`. Allocates a new `ConnectionID` for a
    /// prospective transfer described by `remote_protocol_info` (the peer's
    /// side of the `res@protocolInfo` match) and `direction`.
    ///
    /// This renderer has exactly one real playback pipeline, so unlike a
    /// device that can genuinely serve independent concurrent streams, every
    /// prepared connection is handed back the *same* `AVTransportID`/`RcsID`
    /// (`0`) — there's only one real AVTransport/RenderingControl instance
    /// to point at. What this action still legitimately provides on top of
    /// the always-present connection `0`: capability negotiation up front
    /// (reject an incompatible protocol/direction before `SetAVTransportURI`
    /// is even attempted) and an explicit connection lifecycle a control
    /// point can query and tear down.
    async fn prepare_connection(
        &self,
        remote_protocol_info: String,
        peer_connection_manager: String,
        peer_connection_id: i32,
        direction: ConnectionDirection,
    ) -> Result<PreparedConnection, ServiceError>;

    /// UPnP's `ConnectionComplete()`. Returns whether `connection_id`
    /// existed and was removed; the implicit default connection `0` can
    /// never be removed this way (mirrors
    /// [`InstanceRegistry::remove`](crate::services::instance::InstanceRegistry::remove)'s
    /// treatment of its default instance) and so always reports `false` for
    /// it.
    async fn connection_complete(&self, connection_id: i32) -> Result<bool, ServiceError>;

    /// UPnP's `GetFeatureList()` — required for `ConnectionManager:3`. A
    /// device with no optional CM features (no CLOCKSYNC support here)
    /// still returns a well-formed, empty `Features` document rather than
    /// omitting the action.
    async fn get_feature_list(&self) -> Result<String, ServiceError> {
        Ok(EMPTY_FEATURE_LIST.to_string())
    }
}

/// The most `prepare_connection` will allocate beyond the implicit default
/// connection `0` before returning UPnP error 708 (Connection Table
/// Overflow). This renderer has exactly one real playback pipeline behind
/// every connection regardless of how many are prepared (see
/// `prepare_connection`'s docs), so a small cap keeps `CurrentConnectionIDs`
/// from growing unboundedly if a control point prepares connections it never
/// completes, while still letting a well-behaved control point actually use
/// the action.
const MAX_PREPARED_CONNECTIONS: usize = 1;

pub struct DummyConnectionManager {
    state: Arc<RwLock<ConnectionState>>,
    protocol_info: Vec<ProtocolInfo>,
    // Connections allocated via `prepare_connection`, keyed by ConnectionID.
    // The implicit default connection `0` is never stored here — it's
    // synthesized on demand, exactly as before `prepare_connection` existed,
    // so control points that never call it see unchanged behavior.
    connections: RwLock<HashMap<i32, ConnectionInfo>>,
    next_connection_id: AtomicI32,
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
        "audio/wav" | "audio/x-wav" | "audio/vnd.wave" | "audio/L16" => Some("DLNA.ORG_PN=LPCM"),
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
            connections: RwLock::new(HashMap::new()),
            next_connection_id: AtomicI32::new(1),
        }
    }

    /// Loose compatibility check between a remote `protocolInfo` string
    /// (`PrepareForConnection`'s `RemoteProtocolInfo`, e.g. from a
    /// MediaServer item's `res@protocolInfo`) and one of this
    /// ConnectionManager's own sink `ProtocolInfo` entries. Follows the
    /// 4-field `protocol:network:contentFormat:additionalInfo` shape from
    /// Annex C, treating `*` in either side's protocol/network/mime-type
    /// field as a wildcard match; `additionalInfo` (DLNA profile, etc.) is
    /// deliberately not compared — real-world `additionalInfo` values vary
    /// too much (profile strings, `DLNA.ORG_OP`/`FLAGS` params, plain `*`)
    /// for a strict comparison to be meaningful here.
    fn protocol_info_compatible(local: &ProtocolInfo, remote: &str) -> bool {
        let fields: Vec<&str> = remote.splitn(4, ':').collect();
        let [protocol, network, mime_type, ..] = fields[..] else {
            return false;
        };
        fn matches(local: &str, remote: &str) -> bool {
            local == "*" || remote == "*" || local == remote
        }
        matches(&local.protocol, protocol)
            && matches(&local.network, network)
            && matches(&local.mime_type, mime_type)
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
        // Connection 0 always exists (the fallback every control point uses
        // when it never calls PrepareForConnection), plus anything actually
        // prepared.
        let mut ids: Vec<i32> = self.connections.read().await.keys().copied().collect();
        ids.push(0);
        ids.sort_unstable();
        Ok(ids)
    }

    async fn get_current_connection_info(
        &self,
        connection_id: i32,
    ) -> Result<Option<ConnectionInfo>, ServiceError> {
        if connection_id != 0 {
            return Ok(self.connections.read().await.get(&connection_id).cloned());
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

    async fn prepare_connection(
        &self,
        remote_protocol_info: String,
        peer_connection_manager: String,
        peer_connection_id: i32,
        direction: ConnectionDirection,
    ) -> Result<PreparedConnection, ServiceError> {
        // We're a MediaRenderer: we only ever sink content.
        if direction != ConnectionDirection::Input {
            return Err(ServiceError::Connection(
                ConnectionError::IncompatibleDirection(format!("{:?}", direction)),
            ));
        }

        if !self
            .protocol_info
            .iter()
            .any(|local| Self::protocol_info_compatible(local, &remote_protocol_info))
        {
            return Err(ServiceError::Connection(
                ConnectionError::IncompatibleProtocolInfo(remote_protocol_info.clone()),
            ));
        }

        let mut connections = self.connections.write().await;
        if connections.len() >= MAX_PREPARED_CONNECTIONS {
            return Err(ServiceError::Connection(
                ConnectionError::ConnectionTableOverflow,
            ));
        }

        let connection_id = self.next_connection_id.fetch_add(1, Ordering::Relaxed);
        // Every connection — prepared or implicit — is backed by the same
        // single AVTransport/RenderingControl instance (0); see this
        // method's doc comment on the trait.
        connections.insert(
            connection_id,
            ConnectionInfo {
                connection_id,
                rcs_id: 0,
                av_transport_id: 0,
                protocol_info: remote_protocol_info,
                peer_connection_manager,
                peer_connection_id,
                direction,
                status: ConnectionManagerStatus::Ok,
            },
        );

        Ok(PreparedConnection {
            connection_id,
            av_transport_id: 0,
            rcs_id: 0,
        })
    }

    async fn connection_complete(&self, connection_id: i32) -> Result<bool, ServiceError> {
        if connection_id == 0 {
            // The implicit default connection always exists and can't be
            // torn down through this action.
            return Ok(false);
        }
        Ok(self
            .connections
            .write()
            .await
            .remove(&connection_id)
            .is_some())
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

    #[tokio::test]
    async fn prepare_connection_allocates_a_new_id_aliasing_instance_zero() {
        let manager = DummyConnectionManager::new();
        let prepared = manager
            .prepare_connection(
                "http-get:*:audio/mpeg:*".to_string(),
                String::new(),
                -1,
                ConnectionDirection::Input,
            )
            .await
            .unwrap();

        assert_ne!(prepared.connection_id, 0);
        assert_eq!(prepared.av_transport_id, 0);
        assert_eq!(prepared.rcs_id, 0);

        let mut ids = manager.get_current_connection_ids().await.unwrap();
        ids.sort_unstable();
        assert_eq!(ids, vec![0, prepared.connection_id]);
    }

    #[tokio::test]
    async fn prepare_connection_rejects_output_direction() {
        let manager = DummyConnectionManager::new();
        let err = manager
            .prepare_connection(
                "http-get:*:audio/mpeg:*".to_string(),
                String::new(),
                -1,
                ConnectionDirection::Output,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ServiceError::Connection(ConnectionError::IncompatibleDirection(_))
        ));
    }

    #[tokio::test]
    async fn prepare_connection_rejects_unsupported_protocol_info() {
        let manager = DummyConnectionManager::new();
        let err = manager
            .prepare_connection(
                "http-get:*:video/mp4:*".to_string(),
                String::new(),
                -1,
                ConnectionDirection::Input,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ServiceError::Connection(ConnectionError::IncompatibleProtocolInfo(_))
        ));
    }

    #[tokio::test]
    async fn prepare_connection_overflows_past_the_cap() {
        let manager = DummyConnectionManager::new();
        manager
            .prepare_connection(
                "http-get:*:audio/mpeg:*".to_string(),
                String::new(),
                -1,
                ConnectionDirection::Input,
            )
            .await
            .unwrap();

        let err = manager
            .prepare_connection(
                "http-get:*:audio/mpeg:*".to_string(),
                String::new(),
                -1,
                ConnectionDirection::Input,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ServiceError::Connection(ConnectionError::ConnectionTableOverflow)
        ));
    }

    #[tokio::test]
    async fn connection_complete_removes_a_prepared_connection() {
        let manager = DummyConnectionManager::new();
        let prepared = manager
            .prepare_connection(
                "http-get:*:audio/mpeg:*".to_string(),
                String::new(),
                -1,
                ConnectionDirection::Input,
            )
            .await
            .unwrap();

        assert!(manager
            .connection_complete(prepared.connection_id)
            .await
            .unwrap());
        assert_eq!(manager.get_current_connection_ids().await.unwrap(), vec![0]);
        // Removing again reports "nothing to remove", not an error.
        assert!(!manager
            .connection_complete(prepared.connection_id)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn connection_complete_rejects_the_implicit_default_connection() {
        let manager = DummyConnectionManager::new();
        assert!(!manager.connection_complete(0).await.unwrap());
        assert_eq!(manager.get_current_connection_ids().await.unwrap(), vec![0]);
    }

    #[tokio::test]
    async fn get_feature_list_returns_a_well_formed_empty_document() {
        let manager = DummyConnectionManager::new();
        let feature_list = manager.get_feature_list().await.unwrap();
        assert!(feature_list.contains("<Features"));
        assert!(feature_list.contains("</Features>"));
    }
}
