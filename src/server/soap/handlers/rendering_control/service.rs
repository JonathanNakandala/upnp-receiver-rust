use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::parser::extract_and_parse_soap_body;
use crate::server::soap::types::soap_action::ParsedSoapAction;
use crate::server::soap::types::UPnPService;

use crate::server::soap::types::rendering_control::{
    GetMuteRequest, GetVolumeDBRangeRequest, GetVolumeDBRequest, GetVolumeRequest,
    ListPresetsRequest, RenderingControlAction, SelectPresetRequest, SetMuteRequest,
    SetVolumeDBRequest, SetVolumeRequest,
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

    fn parse_action(
        action: &ParsedSoapAction,
        content: &String,
    ) -> Result<Self::Action, SoapError> {
        let action_name: &str = action.action.as_str();
        match action_name {
            "ListPresets" => {
                let request =
                    extract_and_parse_soap_body::<ListPresetsRequest>(content, action_name)?;
                Ok(RenderingControlAction::ListPresets(request))
            }
            "SelectPreset" => {
                let request =
                    extract_and_parse_soap_body::<SelectPresetRequest>(content, action_name)?;
                Ok(RenderingControlAction::SelectPreset(request))
            }
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
            "SetVolumeDB" => {
                let request =
                    extract_and_parse_soap_body::<SetVolumeDBRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetVolumeDB(request))
            }
            "GetVolumeDB" => {
                let request =
                    extract_and_parse_soap_body::<GetVolumeDBRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetVolumeDB(request))
            }
            "GetVolumeDBRange" => {
                let request =
                    extract_and_parse_soap_body::<GetVolumeDBRangeRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetVolumeDBRange(request))
            }
            _ => Err(SoapError::Soap(SoapProtocolError::NotImplemented(format!(
                "Action not Implemented Yet: {}",
                action_name
            )))),
        }
    }
}
