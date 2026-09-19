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

use crate::server::soap::types::rendering_control::{
    GetMuteRequest, GetVolumeRequest, RenderingControlAction, SetMuteRequest, SetVolumeRequest,
};

pub struct RenderingControlService;

#[async_trait::async_trait]
impl UPnPService for RenderingControlService {
    type Action = RenderingControlAction;

    fn service_type() -> &'static str {
        "urn:schemas-upnp-org:service:RenderingControl:1"
    }

    fn service_id() -> &'static str {
        "RenderingControl"
    }

    fn service_name() -> &'static str {
        "RenderingControl"
    }

    async fn handle_action(
        action: Self::Action,
        renderer: Arc<MediaRenderer>,
    ) -> Result<Response<Body>, SoapError> {
        let response = action.handle(renderer).await?;
        create_soap_response(Self::service_type(), response).map(IntoResponse::into_response)
    }

    fn parse_action(
        action: &ParsedSoapAction,
        content: &String,
    ) -> Result<Self::Action, SoapError> {
        let action_name: &str = action.action.as_str();
        match action_name {
            "SetVolume" => {
                let request =
                    extract_and_parse_soap_body::<SetVolumeRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetVolume(request))
            }
            "SetMute" => {
                let request =
                    extract_and_parse_soap_body::<SetMuteRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetMute(request))
            }
            "GetVolume" => {
                let request =
                    extract_and_parse_soap_body::<GetVolumeRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetVolume(request))
            }
            "GetMute" => {
                let request =
                    extract_and_parse_soap_body::<GetMuteRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetMute(request))
            }
            _ => Err(SoapError::Soap(SoapProtocolError::NotImplemented(format!(
                "Action not Implemented Yet: {}",
                action_name
            )))),
        }
    }
}
