use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::connection_manager::{
    ConnectionManagerAction, ConnectionManagerResponse, GetCurrentConnectionIDsResponse,
    GetCurrentConnectionInfoResponse, GetProtocolInfoResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::MediaRenderer;
use std::sync::Arc;
use tracing::{info, warn};

impl UPnPAction for ConnectionManagerAction {
    type Response = ConnectionManagerResponse;

    fn action_name(&self) -> &'static str {
        match self {
            Self::GetProtocolInfo(_) => "GetProtocolInfo",
            Self::GetCurrentConnectionIDs(_) => "GetCurrentConnectionIDs",
            Self::GetCurrentConnectionInfo(_) => "GetCurrentConnectionInfo",
        }
    }

    fn namespace(&self) -> &'static str {
        "urn:schemas-upnp-org:service:ConnectionManager:1"
    }

    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError> {
        match self {
            Self::GetProtocolInfo(_) => {
                let result = renderer
                    .connection_manager
                    .get_protocol_info()
                    .await
                    .map(|infos| infos.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","));

                match &result {
                    Ok(sink) => info!(sink = %sink, "Handling GetProtocolInfo"),
                    Err(err) => warn!(error = %err, "Handling GetProtocolInfo"),
                }

                let sink = result?;
                Ok(ConnectionManagerResponse::ProtocolInfo(
                    GetProtocolInfoResponse {
                        // We're a MediaRenderer: we only ever sink content, never source it.
                        source: String::new(),
                        sink,
                    },
                ))
            }
            Self::GetCurrentConnectionIDs(_) => {
                let result = renderer
                    .connection_manager
                    .get_current_connection_ids()
                    .await
                    .map(|ids| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(","));

                match &result {
                    Ok(connection_ids) => {
                        info!(connection_ids = %connection_ids, "Handling GetCurrentConnectionIDs")
                    }
                    Err(err) => warn!(error = %err, "Handling GetCurrentConnectionIDs"),
                }

                let connection_ids = result?;
                Ok(ConnectionManagerResponse::ConnectionIDs(
                    GetCurrentConnectionIDsResponse { connection_ids },
                ))
            }
            Self::GetCurrentConnectionInfo(req) => {
                let result = async {
                    renderer
                        .connection_manager
                        .get_current_connection_info(req.connection_id)
                        .await?
                        .ok_or_else(|| {
                            SoapError::Soap(SoapProtocolError::InvalidConnectionReference(
                                format!("Unknown ConnectionID: {}", req.connection_id),
                            ))
                        })
                }
                .await;

                match &result {
                    Ok(info) => info!(
                        connection_id = req.connection_id,
                        rcs_id = info.rcs_id,
                        av_transport_id = info.av_transport_id,
                        "Handling GetCurrentConnectionInfo"
                    ),
                    Err(err) => warn!(
                        connection_id = req.connection_id,
                        error = %err,
                        "Handling GetCurrentConnectionInfo"
                    ),
                }

                let info = result?;
                Ok(ConnectionManagerResponse::ConnectionInfo(
                    GetCurrentConnectionInfoResponse {
                        rcs_id: info.rcs_id,
                        av_transport_id: info.av_transport_id,
                        protocol_info: info.protocol_info,
                        peer_connection_manager: info.peer_connection_manager,
                        peer_connection_id: info.peer_connection_id,
                        direction: info.direction.to_string(),
                        status: info.status.to_string(),
                    },
                ))
            }
        }
    }
}
