use crate::server::eventing::{connection_manager_current_ids_event, EventManager};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::connection_manager::{
    ConnectionCompleteResponse, ConnectionManagerAction, ConnectionManagerResponse,
    GetCurrentConnectionIDsResponse, GetCurrentConnectionInfoResponse, GetFeatureListResponse,
    GetProtocolInfoResponse, PrepareForConnectionResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::types::ConnectionDirection;
use crate::services::MediaRenderer;
use std::sync::Arc;
use tracing::{info, warn};

/// Publishes a `CurrentConnectionIDs` NOTIFY reflecting the connection table
/// right after `PrepareForConnection`/`ConnectionComplete` changes it, so
/// subscribed control points find out immediately instead of waiting for
/// their next `GetCurrentConnectionIDs` poll.
async fn publish_connection_ids(renderer: &MediaRenderer, events: &Arc<EventManager>) {
    let ids = renderer
        .connection_manager
        .get_current_connection_ids()
        .await
        .unwrap_or_default();
    let ids = ids.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
    events.broadcast(connection_manager_current_ids_event(&ids));
}

impl UPnPAction for ConnectionManagerAction {
    type Response = ConnectionManagerResponse;

    fn action_name(&self) -> &'static str {
        match self {
            Self::GetProtocolInfo(_) => "GetProtocolInfo",
            Self::GetCurrentConnectionIDs(_) => "GetCurrentConnectionIDs",
            Self::GetCurrentConnectionInfo(_) => "GetCurrentConnectionInfo",
            Self::PrepareForConnection(_) => "PrepareForConnection",
            Self::ConnectionComplete(_) => "ConnectionComplete",
            Self::GetFeatureList(_) => "GetFeatureList",
        }
    }

    fn namespace(&self) -> &'static str {
        "urn:schemas-upnp-org:service:ConnectionManager:3"
    }

    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError> {
        match self {
            Self::GetProtocolInfo(_) => {
                let result = renderer
                    .connection_manager
                    .get_protocol_info()
                    .await
                    .map(|infos| {
                        infos
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    });

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
                    .map(|ids| {
                        ids.iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    });

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
                            SoapError::Soap(SoapProtocolError::InvalidConnectionReference(format!(
                                "Unknown ConnectionID: {}",
                                req.connection_id
                            )))
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
            Self::PrepareForConnection(req) => {
                let direction = match req.direction.as_str() {
                    "Input" => ConnectionDirection::Input,
                    "Output" => ConnectionDirection::Output,
                    other => {
                        return Err(SoapError::Soap(SoapProtocolError::ParseError(format!(
                            "Invalid Direction: {}",
                            other
                        ))))
                    }
                };

                let result = renderer
                    .connection_manager
                    .prepare_connection(
                        req.remote_protocol_info.clone(),
                        req.peer_connection_manager.clone(),
                        req.peer_connection_id,
                        direction,
                    )
                    .await;

                match &result {
                    Ok(prepared) => info!(
                        connection_id = prepared.connection_id,
                        av_transport_id = prepared.av_transport_id,
                        rcs_id = prepared.rcs_id,
                        "Handling PrepareForConnection"
                    ),
                    Err(err) => warn!(error = %err, "Handling PrepareForConnection"),
                }

                let prepared = result?;
                publish_connection_ids(&renderer, &renderer.connection_manager_events).await;
                Ok(ConnectionManagerResponse::PrepareForConnection(
                    PrepareForConnectionResponse {
                        connection_id: prepared.connection_id,
                        av_transport_id: prepared.av_transport_id,
                        rcs_id: prepared.rcs_id,
                    },
                ))
            }
            Self::ConnectionComplete(req) => {
                let result = async {
                    let removed = renderer
                        .connection_manager
                        .connection_complete(req.connection_id)
                        .await?;
                    if !removed {
                        return Err(SoapError::Soap(
                            SoapProtocolError::InvalidConnectionReference(format!(
                                "Unknown ConnectionID: {}",
                                req.connection_id
                            )),
                        ));
                    }
                    Ok(())
                }
                .await;

                match &result {
                    Ok(()) => info!(
                        connection_id = req.connection_id,
                        "Handling ConnectionComplete"
                    ),
                    Err(err) => warn!(
                        connection_id = req.connection_id,
                        error = %err,
                        "Handling ConnectionComplete"
                    ),
                }

                result?;
                publish_connection_ids(&renderer, &renderer.connection_manager_events).await;
                Ok(ConnectionManagerResponse::ConnectionComplete(
                    ConnectionCompleteResponse {},
                ))
            }
            Self::GetFeatureList(_) => {
                let result = renderer.connection_manager.get_feature_list().await;

                match &result {
                    Ok(_) => info!("Handling GetFeatureList"),
                    Err(err) => warn!(error = %err, "Handling GetFeatureList"),
                }

                let feature_list = result?;
                Ok(ConnectionManagerResponse::FeatureList(
                    GetFeatureListResponse { feature_list },
                ))
            }
        }
    }
}
