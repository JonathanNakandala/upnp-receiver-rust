use super::{
    GetCurrentTransportActionsRequest, GetDeviceCapabilitiesRequest, GetMediaInfoRequest,
    GetPositionInfoRequest, GetTransportInfoRequest, GetTransportSettingsRequest, NextRequest,
    PauseRequest, PlayRequest, PreviousRequest, SeekRequest, SetAVTransportURIRequest,
    SetNextAVTransportURIRequest, SetPlayModeRequest, StopRequest,
};
use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AVTransportAction {
    SetAVTransportURI(SetAVTransportURIRequest),
    SetNextAVTransportURI(SetNextAVTransportURIRequest),
    Play(PlayRequest),
    Pause(PauseRequest),
    Stop(StopRequest),
    Seek(SeekRequest),
    Next(NextRequest),
    Previous(PreviousRequest),
    GetTransportInfo(GetTransportInfoRequest),
    GetMediaInfo(GetMediaInfoRequest),
    GetPositionInfo(GetPositionInfoRequest),
    SetPlayMode(SetPlayModeRequest),
    GetTransportSettings(GetTransportSettingsRequest),
    GetCurrentTransportActions(GetCurrentTransportActionsRequest),
    GetDeviceCapabilities(GetDeviceCapabilitiesRequest),
}
