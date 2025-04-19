use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::parser::extract_and_parse_soap_body;
use crate::server::soap::types::soap_action::ParsedSoapAction;
use crate::server::soap::types::{UPnPAction, UPnPService};
use crate::server::soap::utils::create_soap_response;
use crate::services::MediaRenderer;
use axum::body::Body;
use std::sync::Arc;

use crate::server::soap::types::av_transport::{
    AVTransportAction, GetMediaInfoRequest, GetPositionInfoRequest, GetTransportInfoRequest,
    PauseRequest, PlayRequest, SetAVTransportURIRequest, StopRequest,
};
use axum::http::Response;
use axum::response::IntoResponse;
use tracing::debug;

pub struct AVTransportService;
#[async_trait::async_trait]
impl UPnPService for AVTransportService {
    type Action = AVTransportAction;

    fn service_type() -> &'static str {
        "urn:schemas-upnp-org:service:AVTransport:1"
    }

    fn service_id() -> &'static str {
        "AVTransport"
    }

    fn service_name() -> &'static str {
        "AVTransport"
    }

    async fn handle_action(
        action: Self::Action,
        renderer: Arc<MediaRenderer>,
    ) -> Result<Response<Body>, SoapError> {
        debug!(action = ?action.action_name(), "Handling AVTransport action");

        let response = action.handle(renderer).await?;
        create_soap_response(Self::service_type(), response).map(IntoResponse::into_response)
    }

    fn parse_action(
        action: &ParsedSoapAction,
        content: &String,
    ) -> Result<Self::Action, SoapError> {
        let action_name: &str = action.action.as_str();
        match action_name {
            "SetAVTransportURI" => {
                let request =
                    extract_and_parse_soap_body::<SetAVTransportURIRequest>(content, action_name)?;
                Ok(AVTransportAction::SetAVTransportURI(request))
            }
            "Play" => {
                let request = extract_and_parse_soap_body::<PlayRequest>(content, action_name)?;
                Ok(AVTransportAction::Play(request))
            }
            "Pause" => {
                let request = extract_and_parse_soap_body::<PauseRequest>(content, action_name)?;
                Ok(AVTransportAction::Pause(request))
            }
            "Stop" => {
                let request = extract_and_parse_soap_body::<StopRequest>(content, action_name)?;
                Ok(AVTransportAction::Stop(request))
            }
            "GetTransportInfo" => {
                let request =
                    extract_and_parse_soap_body::<GetTransportInfoRequest>(content, action_name)?;
                Ok(AVTransportAction::GetTransportInfo(request))
            }
            "GetMediaInfo" => {
                let request =
                    extract_and_parse_soap_body::<GetMediaInfoRequest>(content, action_name)?;
                Ok(AVTransportAction::GetMediaInfo(request))
            }
            "GetPositionInfo" => {
                let request =
                    extract_and_parse_soap_body::<GetPositionInfoRequest>(content, action_name)?;
                Ok(AVTransportAction::GetPositionInfo(request))
            }
            _ => Err(SoapError::Soap(SoapProtocolError::NotImplemented(format!(
                "Action not Implemented Yet: {}",
                action_name
            )))),
        }
    }
}
