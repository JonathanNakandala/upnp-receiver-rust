use super::{
    GetMediaInfoRequest, GetPositionInfoRequest, GetTransportInfoRequest, PauseRequest,
    PlayRequest, SetAVTransportURIRequest, StopRequest,
};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use serde::Deserialize;
use tracing::info;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AVTransportAction {
    SetAVTransportURI(SetAVTransportURIRequest),
    Play(PlayRequest),
    Pause(PauseRequest),
    Stop(StopRequest),
    GetTransportInfo(GetTransportInfoRequest),
    GetMediaInfo(GetMediaInfoRequest),
    GetPositionInfo(GetPositionInfoRequest),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AVTransportActionType {
    SetAVTransportURI,
    Play,
    Pause,
    Stop,
    GetTransportInfo,
    GetMediaInfo,
    GetPositionInfo,
}
impl TryFrom<&String> for AVTransportActionType {
    type Error = SoapError;

    fn try_from(action: &String) -> Result<Self, Self::Error> {
        let action = action.trim_matches('"');
        let action_name = action.split('#').last().unwrap_or(action);
        info!(
            action_name = action_name,
            "Matching Action Name with match statement"
        );
        match action_name {
            "SetAVTransportURI" => Ok(Self::SetAVTransportURI),
            "Play" => Ok(Self::Play),
            "Pause" => Ok(Self::Pause),
            "Stop" => Ok(Self::Stop),
            "GetTransportInfo" => Ok(Self::GetTransportInfo),
            "GetMediaInfo" => Ok(Self::GetMediaInfo),
            "GetPositionInfo" => Ok(Self::GetPositionInfo),
            _ => Err(SoapError::Soap(SoapProtocolError::InvalidAction(format!(
                "Unsupported AVTransport action: {}",
                action
            )))),
        }
    }
}
