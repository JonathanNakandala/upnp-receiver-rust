use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::parser::extract_and_parse_soap_body;
use crate::server::soap::types::soap_action::ParsedSoapAction;
use crate::server::soap::types::UPnPService;

use crate::server::soap::types::rendering_control::{
    GetBlueVideoBlackLevelRequest, GetBlueVideoGainRequest, GetBrightnessRequest,
    GetColorTemperatureRequest, GetContrastRequest, GetGreenVideoBlackLevelRequest,
    GetGreenVideoGainRequest, GetHorizontalKeystoneRequest, GetMuteRequest,
    GetRedVideoBlackLevelRequest, GetRedVideoGainRequest, GetSharpnessRequest,
    GetLoudnessRequest, GetVerticalKeystoneRequest, GetVolumeDBRangeRequest, GetVolumeDBRequest,
    GetVolumeRequest, ListPresetsRequest, RenderingControlAction, SelectPresetRequest,
    SetBlueVideoBlackLevelRequest, SetBlueVideoGainRequest, SetBrightnessRequest,
    SetColorTemperatureRequest, SetContrastRequest, SetGreenVideoBlackLevelRequest,
    SetGreenVideoGainRequest, SetHorizontalKeystoneRequest, SetLoudnessRequest, SetMuteRequest,
    SetRedVideoBlackLevelRequest, SetRedVideoGainRequest, SetSharpnessRequest,
    SetVerticalKeystoneRequest, SetVolumeDBRequest, SetVolumeRequest,
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
        content: &str,
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
            "SetBrightness" => {
                let request =
                    extract_and_parse_soap_body::<SetBrightnessRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetBrightness(request))
            }
            "GetBrightness" => {
                let request =
                    extract_and_parse_soap_body::<GetBrightnessRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetBrightness(request))
            }
            "SetContrast" => {
                let request =
                    extract_and_parse_soap_body::<SetContrastRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetContrast(request))
            }
            "GetContrast" => {
                let request =
                    extract_and_parse_soap_body::<GetContrastRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetContrast(request))
            }
            "SetRedVideoGain" => {
                let request =
                    extract_and_parse_soap_body::<SetRedVideoGainRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetRedVideoGain(request))
            }
            "GetRedVideoGain" => {
                let request =
                    extract_and_parse_soap_body::<GetRedVideoGainRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetRedVideoGain(request))
            }
            "SetGreenVideoGain" => {
                let request =
                    extract_and_parse_soap_body::<SetGreenVideoGainRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetGreenVideoGain(request))
            }
            "GetGreenVideoGain" => {
                let request =
                    extract_and_parse_soap_body::<GetGreenVideoGainRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetGreenVideoGain(request))
            }
            "SetBlueVideoGain" => {
                let request =
                    extract_and_parse_soap_body::<SetBlueVideoGainRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetBlueVideoGain(request))
            }
            "GetBlueVideoGain" => {
                let request =
                    extract_and_parse_soap_body::<GetBlueVideoGainRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetBlueVideoGain(request))
            }
            "SetRedVideoBlackLevel" => {
                let request = extract_and_parse_soap_body::<SetRedVideoBlackLevelRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::SetRedVideoBlackLevel(request))
            }
            "GetRedVideoBlackLevel" => {
                let request = extract_and_parse_soap_body::<GetRedVideoBlackLevelRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::GetRedVideoBlackLevel(request))
            }
            "SetGreenVideoBlackLevel" => {
                let request = extract_and_parse_soap_body::<SetGreenVideoBlackLevelRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::SetGreenVideoBlackLevel(request))
            }
            "GetGreenVideoBlackLevel" => {
                let request = extract_and_parse_soap_body::<GetGreenVideoBlackLevelRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::GetGreenVideoBlackLevel(request))
            }
            "SetBlueVideoBlackLevel" => {
                let request = extract_and_parse_soap_body::<SetBlueVideoBlackLevelRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::SetBlueVideoBlackLevel(request))
            }
            "GetBlueVideoBlackLevel" => {
                let request = extract_and_parse_soap_body::<GetBlueVideoBlackLevelRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::GetBlueVideoBlackLevel(request))
            }
            "SetColorTemperature" => {
                let request = extract_and_parse_soap_body::<SetColorTemperatureRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::SetColorTemperature(request))
            }
            "GetColorTemperature" => {
                let request = extract_and_parse_soap_body::<GetColorTemperatureRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::GetColorTemperature(request))
            }
            "SetHorizontalKeystone" => {
                let request = extract_and_parse_soap_body::<SetHorizontalKeystoneRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::SetHorizontalKeystone(request))
            }
            "GetHorizontalKeystone" => {
                let request = extract_and_parse_soap_body::<GetHorizontalKeystoneRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::GetHorizontalKeystone(request))
            }
            "SetVerticalKeystone" => {
                let request = extract_and_parse_soap_body::<SetVerticalKeystoneRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::SetVerticalKeystone(request))
            }
            "GetVerticalKeystone" => {
                let request = extract_and_parse_soap_body::<GetVerticalKeystoneRequest>(
                    content,
                    action_name,
                )?;
                Ok(RenderingControlAction::GetVerticalKeystone(request))
            }
            "SetSharpness" => {
                let request =
                    extract_and_parse_soap_body::<SetSharpnessRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetSharpness(request))
            }
            "GetSharpness" => {
                let request =
                    extract_and_parse_soap_body::<GetSharpnessRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetSharpness(request))
            }
            "SetLoudness" => {
                let request =
                    extract_and_parse_soap_body::<SetLoudnessRequest>(content, action_name)?;
                Ok(RenderingControlAction::SetLoudness(request))
            }
            "GetLoudness" => {
                let request =
                    extract_and_parse_soap_body::<GetLoudnessRequest>(content, action_name)?;
                Ok(RenderingControlAction::GetLoudness(request))
            }
            _ => Err(SoapError::Soap(SoapProtocolError::NotImplemented(format!(
                "Action not Implemented Yet: {}",
                action_name
            )))),
        }
    }
}
