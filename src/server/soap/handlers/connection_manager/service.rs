use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::parser::extract_and_parse_soap_body;
use crate::server::soap::types::soap_action::ParsedSoapAction;
use crate::server::soap::types::{UPnPAction, UPnPService};
use crate::server::soap::utils::create_soap_response;
use crate::services::MediaRenderer;
use axum::body::Body;
use axum::http::Response;
use axum::response::IntoResponse;
use std::sync::Arc;
use tracing::debug;

use crate::server::soap::types::connection_manager::{
    ConnectionManagerAction, GetCurrentConnectionIDsRequest, GetCurrentConnectionInfoRequest,
    GetProtocolInfoRequest,
};

pub struct ConnectionManagerService;

#[async_trait::async_trait]
impl UPnPService for ConnectionManagerService {
    type Action = ConnectionManagerAction;

    fn service_type() -> &'static str {
        "urn:schemas-upnp-org:service:ConnectionManager:1"
    }

    fn service_id() -> &'static str {
        "ConnectionManager"
    }

    fn service_name() -> &'static str {
        "ConnectionManager"
    }

    async fn handle_action(
        action: Self::Action,
        renderer: Arc<MediaRenderer>,
    ) -> Result<Response<Body>, SoapError> {
        debug!(action = ?action.action_name(), "Handling ConnectionManager action");

        let response = action.handle(renderer).await?;
        create_soap_response(Self::service_type(), response).map(IntoResponse::into_response)
    }

    fn parse_action(
        action: &ParsedSoapAction,
        content: &String,
    ) -> Result<Self::Action, SoapError> {
        let action_name: &str = action.action.as_str();
        match action_name {
            "GetProtocolInfo" => {
                let request =
                    extract_and_parse_soap_body::<GetProtocolInfoRequest>(content, action_name)?;
                Ok(ConnectionManagerAction::GetProtocolInfo(request))
            }
            "GetCurrentConnectionIDs" => {
                let request = extract_and_parse_soap_body::<GetCurrentConnectionIDsRequest>(
                    content,
                    action_name,
                )?;
                Ok(ConnectionManagerAction::GetCurrentConnectionIDs(request))
            }
            "GetCurrentConnectionInfo" => {
                let request = extract_and_parse_soap_body::<GetCurrentConnectionInfoRequest>(
                    content,
                    action_name,
                )?;
                Ok(ConnectionManagerAction::GetCurrentConnectionInfo(request))
            }
            _ => Err(SoapError::Soap(SoapProtocolError::NotImplemented(format!(
                "Action not Implemented Yet: {}",
                action_name
            )))),
        }
    }
}
