use crate::services::instance::InstanceId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RenderingControlAction {
    ListPresets(ListPresetsRequest),
    SelectPreset(SelectPresetRequest),
    SetVolume(SetVolumeRequest),
    SetMute(SetMuteRequest),
    GetVolume(GetVolumeRequest),
    GetMute(GetMuteRequest),
    SetVolumeDB(SetVolumeDBRequest),
    GetVolumeDB(GetVolumeDBRequest),
    GetVolumeDBRange(GetVolumeDBRangeRequest),
    SetBrightness(SetBrightnessRequest),
    GetBrightness(GetBrightnessRequest),
    SetContrast(SetContrastRequest),
    GetContrast(GetContrastRequest),
    SetRedVideoGain(SetRedVideoGainRequest),
    GetRedVideoGain(GetRedVideoGainRequest),
    SetGreenVideoGain(SetGreenVideoGainRequest),
    GetGreenVideoGain(GetGreenVideoGainRequest),
    SetBlueVideoGain(SetBlueVideoGainRequest),
    GetBlueVideoGain(GetBlueVideoGainRequest),
    SetRedVideoBlackLevel(SetRedVideoBlackLevelRequest),
    GetRedVideoBlackLevel(GetRedVideoBlackLevelRequest),
    SetGreenVideoBlackLevel(SetGreenVideoBlackLevelRequest),
    GetGreenVideoBlackLevel(GetGreenVideoBlackLevelRequest),
    SetBlueVideoBlackLevel(SetBlueVideoBlackLevelRequest),
    GetBlueVideoBlackLevel(GetBlueVideoBlackLevelRequest),
    SetColorTemperature(SetColorTemperatureRequest),
    GetColorTemperature(GetColorTemperatureRequest),
    SetHorizontalKeystone(SetHorizontalKeystoneRequest),
    GetHorizontalKeystone(GetHorizontalKeystoneRequest),
    SetVerticalKeystone(SetVerticalKeystoneRequest),
    GetVerticalKeystone(GetVerticalKeystoneRequest),
    SetSharpness(SetSharpnessRequest),
    GetSharpness(GetSharpnessRequest),
    SetLoudness(SetLoudnessRequest),
    GetLoudness(GetLoudnessRequest),
    GetStateVariables(GetStateVariablesRequest),
    SetStateVariables(SetStateVariablesRequest),
    GetAllowedTransforms(GetAllowedTransformsRequest),
    GetTransforms(GetTransformsRequest),
    SetTransforms(SetTransformsRequest),
    GetAllAvailableTransforms(GetAllAvailableTransformsRequest),
}

impl RenderingControlAction {
    /// The `InstanceID` the control point sent with this action — see
    /// `AVTransportAction::instance_id` for why this is infallible.
    pub fn instance_id(&self) -> InstanceId {
        let raw = match self {
            Self::ListPresets(r) => r.instance_id,
            Self::SelectPreset(r) => r.instance_id,
            Self::SetVolume(r) => r.instance_id,
            Self::SetMute(r) => r.instance_id,
            Self::GetVolume(r) => r.instance_id,
            Self::GetMute(r) => r.instance_id,
            Self::SetVolumeDB(r) => r.instance_id,
            Self::GetVolumeDB(r) => r.instance_id,
            Self::GetVolumeDBRange(r) => r.instance_id,
            Self::SetBrightness(r) => r.instance_id,
            Self::GetBrightness(r) => r.instance_id,
            Self::SetContrast(r) => r.instance_id,
            Self::GetContrast(r) => r.instance_id,
            Self::SetRedVideoGain(r) => r.instance_id,
            Self::GetRedVideoGain(r) => r.instance_id,
            Self::SetGreenVideoGain(r) => r.instance_id,
            Self::GetGreenVideoGain(r) => r.instance_id,
            Self::SetBlueVideoGain(r) => r.instance_id,
            Self::GetBlueVideoGain(r) => r.instance_id,
            Self::SetRedVideoBlackLevel(r) => r.instance_id,
            Self::GetRedVideoBlackLevel(r) => r.instance_id,
            Self::SetGreenVideoBlackLevel(r) => r.instance_id,
            Self::GetGreenVideoBlackLevel(r) => r.instance_id,
            Self::SetBlueVideoBlackLevel(r) => r.instance_id,
            Self::GetBlueVideoBlackLevel(r) => r.instance_id,
            Self::SetColorTemperature(r) => r.instance_id,
            Self::GetColorTemperature(r) => r.instance_id,
            Self::SetHorizontalKeystone(r) => r.instance_id,
            Self::GetHorizontalKeystone(r) => r.instance_id,
            Self::SetVerticalKeystone(r) => r.instance_id,
            Self::GetVerticalKeystone(r) => r.instance_id,
            Self::SetSharpness(r) => r.instance_id,
            Self::GetSharpness(r) => r.instance_id,
            Self::SetLoudness(r) => r.instance_id,
            Self::GetLoudness(r) => r.instance_id,
            Self::GetStateVariables(r) => r.instance_id,
            Self::SetStateVariables(r) => r.instance_id,
            Self::GetAllowedTransforms(r) => r.instance_id,
            Self::GetTransforms(r) => r.instance_id,
            Self::SetTransforms(r) => r.instance_id,
            // GetAllAvailableTransforms takes no InstanceID at all (see
            // Table 114 in the spec) - it reports this device's whole
            // transform catalog, not anything instance-specific, so there's
            // nothing to resolve here. `RenderingControlAction::handle`
            // still resolves a `control` via this value, so it needs to be
            // a real instance - `InstanceId::DEFAULT`'s raw value always is.
            Self::GetAllAvailableTransforms(_) => InstanceId::DEFAULT.get(),
        };
        InstanceId::new(raw)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListPresetsRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SelectPresetRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub preset_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVolumeRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
    pub desired_volume: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetMuteRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
    pub desired_mute: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMuteRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVolumeDBRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
    pub desired_volume: i16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeDBRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeDBRangeRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
}

// Brightness/Contrast take no Channel argument (Table 32/34/36/38 in the
// RenderingControl spec) - unlike Volume/Mute/VolumeDB, they're a display
// setting, not per audio-channel.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetBrightnessRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_brightness: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBrightnessRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetContrastRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_contrast: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetContrastRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

// RedVideoGain/GreenVideoGain/BlueVideoGain/*BlackLevel/ColorTemperature
// take no Channel argument either - same reasoning as Brightness/Contrast.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetRedVideoGainRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_red_video_gain: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetRedVideoGainRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetGreenVideoGainRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_green_video_gain: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetGreenVideoGainRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetBlueVideoGainRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_blue_video_gain: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBlueVideoGainRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetRedVideoBlackLevelRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_red_video_black_level: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetRedVideoBlackLevelRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetGreenVideoBlackLevelRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_green_video_black_level: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetGreenVideoBlackLevelRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetBlueVideoBlackLevelRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_blue_video_black_level: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBlueVideoBlackLevelRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetColorTemperatureRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_color_temperature: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetColorTemperatureRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

// HorizontalKeystone/VerticalKeystone take no Channel argument either, same
// reasoning as the rest of this group - and are signed (`i2`/`i16`), unlike
// every other picture state variable here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetHorizontalKeystoneRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_horizontal_keystone: i16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetHorizontalKeystoneRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVerticalKeystoneRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_vertical_keystone: i16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVerticalKeystoneRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

// Sharpness takes no Channel argument either, same reasoning as the rest
// of this group.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetSharpnessRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_sharpness: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetSharpnessRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

// Loudness carries a `Channel` argument like Volume/Mute above - see
// `action.rs`'s `validate_channel` for why only `"Master"` is accepted.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetLoudnessRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
    pub desired_loudness: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetLoudnessRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub channel: String,
}

// A generic reflection action, not tied to one state variable - takes a CSV
// of names (or "*" for all of them) and returns their current values as one
// XML blob. See action.rs's ALL_STATE_VARIABLE_NAMES/state_variable_value
// for what it actually reports.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetStateVariablesRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub state_variable_list: String,
}

/// The write side of `GetStateVariables` above - replaces the named state
/// variables' current values from `state_variable_value_pairs`.
/// `rendering_control_udn`/`service_type`/`service_id` are a compatibility
/// check (5.4.38: this device must recognize itself in all three before any
/// value gets applied) - see action.rs's `SERVICE_TYPE`/`SERVICE_ID` and how
/// `rendering_control_udn` is checked against `MediaRenderer::unique_id`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetStateVariablesRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    #[serde(rename = "RenderingControlUDN")]
    pub rendering_control_udn: String,
    pub service_type: String,
    pub service_id: String,
    pub state_variable_value_pairs: String,
}

// Annex B.21 "legacy compatible transforms" - the same reflection idea as
// GetStateVariables above, but viewed as named `transform` elements instead
// of `stateVariable` ones, covering only the 16 transforms Annex B.21
// defines as one-to-one aliases of actions this renderer already
// implements. See action.rs's TRANSFORM_DEFS/apply_transform for the actual
// name<->action mapping. The spec requires these four actions be
// implemented as a set (5.4.39-41, 5.4.45).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetAllowedTransformsRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetTransformsRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetTransformsRequest {
    #[serde(rename = "InstanceID")]
    pub instance_id: u32,
    pub desired_transform_values: String,
}

/// No `InstanceID` - see this action's Table 114 in the spec; it reports
/// the device's whole transform catalog, not anything instance-specific.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetAllAvailableTransformsRequest {}

// Add response types
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeResponse {
    pub current_volume: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetMuteResponse {
    pub current_mute: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListPresetsResponse {
    pub current_preset_name_list: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SelectPresetResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVolumeResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetMuteResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeDBResponse {
    pub current_volume: i16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVolumeDBResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVolumeDBRangeResponse {
    pub min_value: i16,
    pub max_value: i16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetBrightnessResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBrightnessResponse {
    pub current_brightness: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetContrastResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetContrastResponse {
    pub current_contrast: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetRedVideoGainResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetRedVideoGainResponse {
    pub current_red_video_gain: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetGreenVideoGainResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetGreenVideoGainResponse {
    pub current_green_video_gain: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetBlueVideoGainResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBlueVideoGainResponse {
    pub current_blue_video_gain: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetRedVideoBlackLevelResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetRedVideoBlackLevelResponse {
    pub current_red_video_black_level: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetGreenVideoBlackLevelResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetGreenVideoBlackLevelResponse {
    pub current_green_video_black_level: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetBlueVideoBlackLevelResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetBlueVideoBlackLevelResponse {
    pub current_blue_video_black_level: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetColorTemperatureResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetColorTemperatureResponse {
    pub current_color_temperature: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetHorizontalKeystoneResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetHorizontalKeystoneResponse {
    pub current_horizontal_keystone: i16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetVerticalKeystoneResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetVerticalKeystoneResponse {
    pub current_vertical_keystone: i16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetSharpnessResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetSharpnessResponse {
    pub current_sharpness: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetLoudnessResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetLoudnessResponse {
    pub current_loudness: bool,
}

/// `state_variable_value_pairs` is the already-built `<stateVariableValuePairs>`
/// XML document as a plain string - `quick_xml::se::to_string` escapes it
/// like any other string field when this response gets embedded in the SOAP
/// envelope, the same double-escaping `LastChange` NOTIFY bodies rely on
/// (see `server::eventing::last_change`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetStateVariablesResponse {
    pub state_variable_value_pairs: String,
}

/// `state_variable_list` is the CSV of names `SetStateVariables` actually
/// applied - a subset of what was requested only if the request named
/// nothing invalid (an all-or-nothing rejection happens before anything is
/// applied - see action.rs).
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetStateVariablesResponse {
    pub state_variable_list: String,
}

/// `current_allowed_transform_settings` is the already-built `<TransformList>`
/// XML document as a plain string - same double-escaping-on-the-wire
/// reasoning as `GetStateVariablesResponse` above.
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetAllowedTransformsResponse {
    pub current_allowed_transform_settings: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetTransformsResponse {
    pub current_transform_values: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetTransformsResponse {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetAllAvailableTransformsResponse {
    pub all_allowed_transform_settings: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RenderingControlResponse {
    ListPresets(ListPresetsResponse),
    SelectPreset(SelectPresetResponse),
    SetVolume(SetVolumeResponse),
    SetMute(SetMuteResponse),
    Volume(GetVolumeResponse),
    Mute(GetMuteResponse),
    VolumeDB(GetVolumeDBResponse),
    SetVolumeDB(SetVolumeDBResponse),
    VolumeDBRange(GetVolumeDBRangeResponse),
    SetBrightness(SetBrightnessResponse),
    Brightness(GetBrightnessResponse),
    SetContrast(SetContrastResponse),
    Contrast(GetContrastResponse),
    SetRedVideoGain(SetRedVideoGainResponse),
    RedVideoGain(GetRedVideoGainResponse),
    SetGreenVideoGain(SetGreenVideoGainResponse),
    GreenVideoGain(GetGreenVideoGainResponse),
    SetBlueVideoGain(SetBlueVideoGainResponse),
    BlueVideoGain(GetBlueVideoGainResponse),
    SetRedVideoBlackLevel(SetRedVideoBlackLevelResponse),
    RedVideoBlackLevel(GetRedVideoBlackLevelResponse),
    SetGreenVideoBlackLevel(SetGreenVideoBlackLevelResponse),
    GreenVideoBlackLevel(GetGreenVideoBlackLevelResponse),
    SetBlueVideoBlackLevel(SetBlueVideoBlackLevelResponse),
    BlueVideoBlackLevel(GetBlueVideoBlackLevelResponse),
    SetColorTemperature(SetColorTemperatureResponse),
    ColorTemperature(GetColorTemperatureResponse),
    SetHorizontalKeystone(SetHorizontalKeystoneResponse),
    HorizontalKeystone(GetHorizontalKeystoneResponse),
    SetVerticalKeystone(SetVerticalKeystoneResponse),
    VerticalKeystone(GetVerticalKeystoneResponse),
    SetSharpness(SetSharpnessResponse),
    Sharpness(GetSharpnessResponse),
    SetLoudness(SetLoudnessResponse),
    Loudness(GetLoudnessResponse),
    StateVariables(GetStateVariablesResponse),
    SetStateVariables(SetStateVariablesResponse),
    AllowedTransforms(GetAllowedTransformsResponse),
    Transforms(GetTransformsResponse),
    SetTransforms(SetTransformsResponse),
    AllAvailableTransforms(GetAllAvailableTransformsResponse),
}
