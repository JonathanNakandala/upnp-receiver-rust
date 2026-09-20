use super::{
    GetCurrentTransportActionsRequest, GetDeviceCapabilitiesRequest, GetMediaInfoRequest,
    GetPositionInfoRequest, GetTransportInfoRequest, GetTransportSettingsRequest, NextRequest,
    PauseRequest, PlayRequest, PreviousRequest, SeekRequest, SetAVTransportURIRequest,
    SetNextAVTransportURIRequest, SetPlayModeRequest, StopRequest,
};
use crate::services::instance::InstanceId;
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

impl AVTransportAction {
    /// The `InstanceID` the control point sent with this action — every
    /// AVTransport action carries one, so this is a plain field read, not a
    /// fallible lookup (resolving it against the actual set of live
    /// instances happens separately, in the SOAP handler).
    pub fn instance_id(&self) -> InstanceId {
        let raw = match self {
            Self::SetAVTransportURI(r) => r.instance_id,
            Self::SetNextAVTransportURI(r) => r.instance_id,
            Self::Play(r) => r.instance_id,
            Self::Pause(r) => r.instance_id,
            Self::Stop(r) => r.instance_id,
            Self::Seek(r) => r.instance_id,
            Self::Next(r) => r.instance_id,
            Self::Previous(r) => r.instance_id,
            Self::GetTransportInfo(r) => r.instance_id,
            Self::GetMediaInfo(r) => r.instance_id,
            Self::GetPositionInfo(r) => r.instance_id,
            Self::SetPlayMode(r) => r.instance_id,
            Self::GetTransportSettings(r) => r.instance_id,
            Self::GetCurrentTransportActions(r) => r.instance_id,
            Self::GetDeviceCapabilities(r) => r.instance_id,
        };
        InstanceId::new(raw)
    }
}
