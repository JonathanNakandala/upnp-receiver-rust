use super::{
    GetMediaInfoRequest, GetPositionInfoRequest, GetTransportInfoRequest,
    GetTransportSettingsRequest, PauseRequest, PlayRequest, SeekRequest, SetAVTransportURIRequest,
    SetNextAVTransportURIRequest, SetPlayModeRequest, StopRequest,
};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use serde::Deserialize;
use tracing::info;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AVTransportAction {
    SetAVTransportURI(SetAVTransportURIRequest),
    SetNextAVTransportURI(SetNextAVTransportURIRequest),
    Play(PlayRequest),
    Pause(PauseRequest),
    Stop(StopRequest),
    Seek(SeekRequest),
    GetTransportInfo(GetTransportInfoRequest),
    GetMediaInfo(GetMediaInfoRequest),
    GetPositionInfo(GetPositionInfoRequest),
    SetPlayMode(SetPlayModeRequest),
    GetTransportSettings(GetTransportSettingsRequest),
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AVTransportActionType {
    SetAVTransportURI,
    SetNextAVTransportURI,
    Play,
    Pause,
    Stop,
    Seek,
    GetTransportInfo,
    GetMediaInfo,
    GetPositionInfo,
    SetPlayMode,
    GetTransportSettings,
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
            "SetNextAVTransportURI" => Ok(Self::SetNextAVTransportURI),
            "Play" => Ok(Self::Play),
            "Pause" => Ok(Self::Pause),
            "Stop" => Ok(Self::Stop),
            "Seek" => Ok(Self::Seek),
            "GetTransportInfo" => Ok(Self::GetTransportInfo),
            "GetMediaInfo" => Ok(Self::GetMediaInfo),
            "GetPositionInfo" => Ok(Self::GetPositionInfo),
            "SetPlayMode" => Ok(Self::SetPlayMode),
            "GetTransportSettings" => Ok(Self::GetTransportSettings),
            _ => Err(SoapError::Soap(SoapProtocolError::InvalidAction(format!(
                "Unsupported AVTransport action: {}",
                action
            )))),
        }
    }
}
