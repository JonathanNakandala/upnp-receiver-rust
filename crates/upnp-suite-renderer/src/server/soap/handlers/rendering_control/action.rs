use crate::server::eventing::{rendering_control_last_change, EventManager, RenderingControlEventState};
use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::rendering_control::{
    GetAllAvailableTransformsResponse, GetAllowedTransformsResponse,
    GetBlueVideoBlackLevelResponse, GetBlueVideoGainResponse, GetBrightnessResponse,
    GetColorTemperatureResponse, GetContrastResponse, GetGreenVideoBlackLevelResponse,
    GetGreenVideoGainResponse, GetHorizontalKeystoneResponse, GetLoudnessResponse, GetMuteResponse,
    GetRedVideoBlackLevelResponse, GetRedVideoGainResponse, GetSharpnessResponse,
    GetStateVariablesResponse, GetTransformsResponse, GetVerticalKeystoneResponse,
    GetVolumeDBRangeResponse, GetVolumeDBResponse, GetVolumeResponse, ListPresetsResponse,
    RenderingControlAction, RenderingControlResponse, SelectPresetResponse,
    SetBlueVideoBlackLevelResponse, SetBlueVideoGainResponse, SetBrightnessResponse,
    SetColorTemperatureResponse, SetContrastResponse, SetGreenVideoBlackLevelResponse,
    SetGreenVideoGainResponse, SetHorizontalKeystoneResponse, SetLoudnessResponse, SetMuteResponse,
    SetRedVideoBlackLevelResponse, SetRedVideoGainResponse, SetSharpnessResponse,
    SetStateVariablesResponse, SetTransformsResponse, SetVerticalKeystoneResponse,
    SetVolumeDBResponse, SetVolumeResponse,
};
use crate::server::soap::types::UPnPAction;
use crate::services::instance::InstanceId;
use crate::services::rendering_control::RenderingControl;
use crate::services::MediaRenderer;
use quick_xml::events::Event;
use std::sync::Arc;
use tracing::{info, warn};

/// Publishes a `LastChange` NOTIFY reflecting current picture/audio state
/// right after a state-changing action succeeds, so subscribed control
/// points find out immediately instead of waiting for their next poll.
/// `instance_id`/`control` are the instance the triggering action was
/// actually resolved against — see `RenderingControlAction::handle` below.
async fn publish_rendering_state(
    instance_id: InstanceId,
    control: &dyn RenderingControl,
    events: &Arc<EventManager>,
) {
    let volume = control.get_volume().await.unwrap_or(0);
    let muted = control.get_mute().await.unwrap_or(false);
    let brightness = control.get_brightness().await.unwrap_or(50);
    let contrast = control.get_contrast().await.unwrap_or(50);
    let red_gain = control.get_red_video_gain().await.unwrap_or(500);
    let green_gain = control.get_green_video_gain().await.unwrap_or(500);
    let blue_gain = control.get_blue_video_gain().await.unwrap_or(500);
    let red_black_level = control.get_red_video_black_level().await.unwrap_or(500);
    let green_black_level = control.get_green_video_black_level().await.unwrap_or(500);
    let blue_black_level = control.get_blue_video_black_level().await.unwrap_or(500);
    let color_temperature = control.get_color_temperature().await.unwrap_or(50);
    let horizontal_keystone = control.get_horizontal_keystone().await.unwrap_or(0);
    let vertical_keystone = control.get_vertical_keystone().await.unwrap_or(0);
    let sharpness = control.get_sharpness().await.unwrap_or(0);
    let loudness = control.get_loudness().await.unwrap_or(false);
    events.broadcast(rendering_control_last_change(
        instance_id,
        RenderingControlEventState {
            volume,
            muted,
            brightness,
            contrast,
            red_gain,
            green_gain,
            blue_gain,
            red_black_level,
            green_black_level,
            blue_black_level,
            color_temperature,
            horizontal_keystone,
            vertical_keystone,
            sharpness,
            loudness,
        },
    ));
}

/// This service's identity as declared in `device.xml`'s `<serviceList>` -
/// used both for the SOAP response namespace below and, unlike everywhere
/// else in this file, as data `SetStateVariables` actually compares a
/// control point's claims against (5.4.38's UDN/ServiceType/ServiceId
/// compatibility check - see its handling in `handle()`).
pub(super) const SERVICE_TYPE: &str = "urn:schemas-upnp-org:service:RenderingControl:3";
const SERVICE_ID: &str = "urn:upnp-org:serviceId:RenderingControl";

/// UPnP's `A_ARG_TYPE_Channel` only ever allows `"Master"` here (see the
/// SCPD's `allowedValueList` for it) — this renderer has no concept of
/// individual audio channels to apply a per-channel setting to.
fn validate_channel(channel: &str) -> Result<(), SoapError> {
    if channel != "Master" {
        return Err(SoapError::Soap(SoapProtocolError::InvalidChannel(
            channel.to_string(),
        )));
    }
    Ok(())
}

/// Every state variable `GetStateVariables` can report, in the order used
/// when a control point asks for all of them via `"*"` — mirrors
/// `RenderingControlEventState`'s field order. Excludes `LastChange` and
/// `PresetNameList` per the spec's own exclusion list for this action (the
/// v3 Transform state variables it also excludes don't exist on this
/// renderer at all, so there's nothing further to leave out).
const ALL_STATE_VARIABLE_NAMES: &[&str] = &[
    "Volume",
    "VolumeDB",
    "Mute",
    "Brightness",
    "Contrast",
    "RedVideoGain",
    "GreenVideoGain",
    "BlueVideoGain",
    "RedVideoBlackLevel",
    "GreenVideoBlackLevel",
    "BlueVideoBlackLevel",
    "ColorTemperature",
    "HorizontalKeystone",
    "VerticalKeystone",
    "Sharpness",
    "Loudness",
];

fn bool_str(value: bool) -> String {
    if value { "1" } else { "0" }.to_string()
}

/// Looks up one state variable's current wire value, plus whether it
/// carries a `channel="Master"` attribute — mirrors
/// `rendering_control_last_change`'s per-variable channel handling exactly
/// (`Volume`/`VolumeDB`/`Mute`/`Loudness` only). `None` means `name` isn't
/// one of [`ALL_STATE_VARIABLE_NAMES`], which the caller turns into UPnP
/// error 704 rather than silently skipping.
async fn state_variable_value(
    control: &dyn RenderingControl,
    name: &str,
) -> Option<Result<(String, bool), SoapError>> {
    let with_channel = |v: Result<String, _>| v.map(|v| (v, true)).map_err(SoapError::from);
    let no_channel = |v: Result<String, _>| v.map(|v| (v, false)).map_err(SoapError::from);
    Some(match name {
        "Volume" => with_channel(control.get_volume().await.map(|v| v.to_string())),
        "VolumeDB" => with_channel(control.get_volume_db().await.map(|v| v.to_string())),
        "Mute" => with_channel(control.get_mute().await.map(bool_str)),
        "Brightness" => no_channel(control.get_brightness().await.map(|v| v.to_string())),
        "Contrast" => no_channel(control.get_contrast().await.map(|v| v.to_string())),
        "RedVideoGain" => no_channel(control.get_red_video_gain().await.map(|v| v.to_string())),
        "GreenVideoGain" => {
            no_channel(control.get_green_video_gain().await.map(|v| v.to_string()))
        }
        "BlueVideoGain" => no_channel(control.get_blue_video_gain().await.map(|v| v.to_string())),
        "RedVideoBlackLevel" => no_channel(
            control
                .get_red_video_black_level()
                .await
                .map(|v| v.to_string()),
        ),
        "GreenVideoBlackLevel" => no_channel(
            control
                .get_green_video_black_level()
                .await
                .map(|v| v.to_string()),
        ),
        "BlueVideoBlackLevel" => no_channel(
            control
                .get_blue_video_black_level()
                .await
                .map(|v| v.to_string()),
        ),
        "ColorTemperature" => {
            no_channel(control.get_color_temperature().await.map(|v| v.to_string()))
        }
        "HorizontalKeystone" => no_channel(
            control
                .get_horizontal_keystone()
                .await
                .map(|v| v.to_string()),
        ),
        "VerticalKeystone" => {
            no_channel(control.get_vertical_keystone().await.map(|v| v.to_string()))
        }
        "Sharpness" => no_channel(control.get_sharpness().await.map(|v| v.to_string())),
        "Loudness" => with_channel(control.get_loudness().await.map(bool_str)),
        _ => return None,
    })
}

/// A transform's numeric range or, for the two boolean ones, that it's
/// boolean - drives [`allowed_transform_settings_xml`]'s output.
enum TransformValueKind {
    Range { min: i32, max: i32, step: i32 },
    Boolean,
}

/// One entry per Annex B.21 "legacy compatible transform" this renderer
/// aliases - every action it implements that Annex B.21 gives a transform
/// name for, and nothing else (this renderer doesn't implement e.g.
/// Rotation/3DSettings, so those transforms don't exist here). `Volume`/
/// `VolumeDB`/`Mute`/`Loudness` embed their channel in the transform name
/// itself per B.21.1-B.21.4 (e.g. `Volume_Master`) rather than taking it as
/// a separate argument the way the underlying actions do - this renderer
/// only ever has a `Master` channel, so `_Master` is the only suffix that
/// exists. Ranges mirror this renderer's own validation
/// (`services::rendering_control::validate_range`/`validate_percent`/
/// `validate_signed_range`), not just what Annex B.21 itself specifies
/// (which leaves most maxima vendor-defined).
struct TransformDef {
    name: &'static str,
    friendly_name: &'static str,
    kind: TransformValueKind,
}

const TRANSFORM_DEFS: &[TransformDef] = &[
    TransformDef {
        name: "Volume_Master",
        friendly_name: "Volume",
        kind: TransformValueKind::Range { min: 0, max: 100, step: 1 },
    },
    TransformDef {
        name: "VolumeDB_Master",
        friendly_name: "Volume (dB)",
        kind: TransformValueKind::Range {
            min: crate::services::rendering_control::VOLUME_DB_MIN as i32,
            max: crate::services::rendering_control::VOLUME_DB_MAX as i32,
            step: 1,
        },
    },
    TransformDef { name: "Mute_Master", friendly_name: "Mute", kind: TransformValueKind::Boolean },
    TransformDef {
        name: "Loudness_Master",
        friendly_name: "Loudness",
        kind: TransformValueKind::Boolean,
    },
    TransformDef {
        name: "Brightness",
        friendly_name: "Brightness",
        kind: TransformValueKind::Range { min: 0, max: 100, step: 1 },
    },
    TransformDef {
        name: "Sharpness",
        friendly_name: "Sharpness",
        kind: TransformValueKind::Range { min: 0, max: 100, step: 1 },
    },
    TransformDef {
        name: "Contrast",
        friendly_name: "Contrast",
        kind: TransformValueKind::Range { min: 0, max: 100, step: 1 },
    },
    TransformDef {
        name: "RedVideoGain",
        friendly_name: "Red video gain",
        kind: TransformValueKind::Range { min: 0, max: 1000, step: 1 },
    },
    TransformDef {
        name: "GreenVideoGain",
        friendly_name: "Green video gain",
        kind: TransformValueKind::Range { min: 0, max: 1000, step: 1 },
    },
    TransformDef {
        name: "BlueVideoGain",
        friendly_name: "Blue video gain",
        kind: TransformValueKind::Range { min: 0, max: 1000, step: 1 },
    },
    TransformDef {
        name: "RedVideoBlackLevel",
        friendly_name: "Red video black level",
        kind: TransformValueKind::Range { min: 0, max: 1000, step: 1 },
    },
    TransformDef {
        name: "GreenVideoBlackLevel",
        friendly_name: "Green video black level",
        kind: TransformValueKind::Range { min: 0, max: 1000, step: 1 },
    },
    TransformDef {
        name: "BlueVideoBlackLevel",
        friendly_name: "Blue video black level",
        kind: TransformValueKind::Range { min: 0, max: 1000, step: 1 },
    },
    TransformDef {
        name: "ColorTemperature",
        friendly_name: "Color temperature",
        kind: TransformValueKind::Range { min: 0, max: 100, step: 1 },
    },
    TransformDef {
        name: "HorizontalKeystone",
        friendly_name: "Horizontal keystone",
        kind: TransformValueKind::Range { min: -50, max: 50, step: 1 },
    },
    TransformDef {
        name: "VerticalKeystone",
        friendly_name: "Vertical keystone",
        kind: TransformValueKind::Range { min: -50, max: 50, step: 1 },
    },
];

/// The [`state_variable_value`] name a transform name aliases - strips the
/// `_Master` suffix Annex B.21.1-B.21.4 bake into `Volume`/`VolumeDB`/
/// `Mute`/`Loudness`'s transform names; every other transform name here is
/// already identical to its state variable name.
fn transform_base_name(transform_name: &str) -> Option<&str> {
    match transform_name {
        "Volume_Master" => Some("Volume"),
        "VolumeDB_Master" => Some("VolumeDB"),
        "Mute_Master" => Some("Mute"),
        "Loudness_Master" => Some("Loudness"),
        name if TRANSFORM_DEFS.iter().any(|d| d.name == name) => Some(name),
        _ => None,
    }
}

/// [`GetTransforms`]'s per-transform value lookup - reuses
/// [`state_variable_value`] (a transform's current value is always the same
/// as its aliased state variable's), just discarding the `channel`
/// attribute that reflection dialect carries and this one doesn't (a
/// transform's `Master` channel is already baked into its name, not a
/// separate attribute - see `current_transform_settings_xml`).
async fn transform_value(
    control: &dyn RenderingControl,
    transform_name: &str,
) -> Option<Result<String, SoapError>> {
    let base = transform_base_name(transform_name)?;
    match state_variable_value(control, base).await {
        Some(Ok((value, _has_channel))) => Some(Ok(value)),
        Some(Err(err)) => Some(Err(err)),
        None => None,
    }
}

/// Why [`apply_transform`] didn't apply a requested transform - distinct
/// UPnP error codes per 5.4.41.4 (711 vs. 712), and per that same subclause
/// `SetTransforms` keeps going and applies whatever it can rather than
/// aborting on the first failure.
enum TransformApplyError {
    /// Not one of [`TRANSFORM_DEFS`] - UPnP error 711.
    UnknownTransform,
    /// A known transform, but a value it couldn't parse or that its
    /// underlying `set_xxx` rejected (out of range) - UPnP error 712.
    UnsupportedValue,
}

/// Applies one parsed `(name, value)` pair from a `SetTransforms` request by
/// calling the same `RenderingControl` setter the aliased action would.
async fn apply_transform(
    control: &dyn RenderingControl,
    name: &str,
    value: &str,
) -> Result<(), TransformApplyError> {
    let range = |v: &str| v.parse::<u16>().map_err(|_| TransformApplyError::UnsupportedValue);
    let signed = |v: &str| v.parse::<i16>().map_err(|_| TransformApplyError::UnsupportedValue);
    let boolean = |v: &str| match v {
        "1" | "true" | "True" => Ok(true),
        "0" | "false" | "False" => Ok(false),
        _ => Err(TransformApplyError::UnsupportedValue),
    };

    let result = match name {
        "Volume_Master" => control.set_volume(range(value)?).await,
        "VolumeDB_Master" => control.set_volume_db(signed(value)?).await,
        "Mute_Master" => control.set_mute(boolean(value)?).await,
        "Loudness_Master" => control.set_loudness(boolean(value)?).await,
        "Brightness" => control.set_brightness(range(value)?).await,
        "Contrast" => control.set_contrast(range(value)?).await,
        "Sharpness" => control.set_sharpness(range(value)?).await,
        "RedVideoGain" => control.set_red_video_gain(range(value)?).await,
        "GreenVideoGain" => control.set_green_video_gain(range(value)?).await,
        "BlueVideoGain" => control.set_blue_video_gain(range(value)?).await,
        "RedVideoBlackLevel" => control.set_red_video_black_level(range(value)?).await,
        "GreenVideoBlackLevel" => control.set_green_video_black_level(range(value)?).await,
        "BlueVideoBlackLevel" => control.set_blue_video_black_level(range(value)?).await,
        "ColorTemperature" => control.set_color_temperature(range(value)?).await,
        "HorizontalKeystone" => control.set_horizontal_keystone(signed(value)?).await,
        "VerticalKeystone" => control.set_vertical_keystone(signed(value)?).await,
        _ => return Err(TransformApplyError::UnknownTransform),
    };
    result.map_err(|_| TransformApplyError::UnsupportedValue)
}

/// Builds `GetAllowedTransforms`/`GetAllAvailableTransforms`'s
/// `<TransformList>` XML document from [`TRANSFORM_DEFS`] - static and
/// identical between the two actions, since this renderer doesn't restrict
/// which transforms apply by InstanceID or content type the way the spec
/// allows.
fn allowed_transform_settings_xml() -> String {
    let mut transforms = String::new();
    for def in TRANSFORM_DEFS {
        match def.kind {
            TransformValueKind::Range { min, max, step } => {
                transforms.push_str(&format!(
                    r#"<transform name="{name}" shared="0"><friendlyName>{friendly}</friendlyName><allowedValueRange><minimum>{min}</minimum><maximum>{max}</maximum><step>{step}</step></allowedValueRange></transform>"#,
                    name = def.name,
                    friendly = def.friendly_name,
                ));
            }
            TransformValueKind::Boolean => {
                transforms.push_str(&format!(
                    r#"<transform name="{name}" shared="0"><friendlyName>{friendly}</friendlyName><allowedValueList><allowedValue>0</allowedValue><allowedValue>1</allowedValue></allowedValueList></transform>"#,
                    name = def.name,
                    friendly = def.friendly_name,
                ));
            }
        }
    }
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<TransformList xmlns="urn:schemas-upnp-org:av:AllowedTransformSettings">"#,
            "{transforms}",
            r#"</TransformList>"#
        ),
        transforms = transforms,
    )
}

/// Builds `GetTransforms`'s `<TransformSettings>` XML document - current
/// values of every transform in [`TRANSFORM_DEFS`], via [`transform_value`].
async fn current_transform_settings_xml(control: &dyn RenderingControl) -> Result<String, SoapError> {
    let mut transforms = String::new();
    for def in TRANSFORM_DEFS {
        let value = match transform_value(control, def.name).await {
            Some(result) => result?,
            // Unreachable: every TRANSFORM_DEFS name round-trips through
            // transform_base_name by construction.
            None => continue,
        };
        transforms.push_str(&format!(
            r#"<transform name="{name}"><value>{value}</value></transform>"#,
            name = def.name,
        ));
    }
    Ok(format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<TransformSettings xmlns="urn:schemas-upnp-org:av:TransformSettings">"#,
            "{transforms}",
            r#"</TransformSettings>"#
        ),
        transforms = transforms,
    ))
}

fn local_tag(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    s.rsplit(':').next().unwrap_or(s)
}

/// Parses `SetTransforms`'s `DesiredTransformValues` argument: each
/// `<transform name="...">`'s first `<value>` child, in document order (per
/// 5.4.41, transforms are applied in the order the control point sent
/// them). None of the transforms in [`TRANSFORM_DEFS`] are multi-value, so
/// a `<value index="N">` sibling beyond the first is ignored rather than
/// supported.
fn parse_transform_settings(xml: &str) -> Result<Vec<(String, String)>, SoapError> {
    let mut reader = quick_xml::reader::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut pairs = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_value: Option<String> = None;
    let mut in_value = false;

    loop {
        let event = reader.read_event().map_err(|e| {
            SoapError::Soap(SoapProtocolError::ParseError(format!(
                "Ill-formed DesiredTransformValues: {e}"
            )))
        })?;
        match event {
            Event::Start(e) => {
                let tag = local_tag(e.name().into_inner()).to_string();
                if tag == "transform" {
                    current_name = e
                        .attributes()
                        .flatten()
                        .find(|a| local_tag(a.key.into_inner()) == "name")
                        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()));
                    current_value = None;
                } else if tag == "value" && current_value.is_none() {
                    in_value = true;
                }
            }
            Event::Text(t) if in_value => {
                current_value = Some(t.unescape().map_err(|e| {
                    SoapError::Soap(SoapProtocolError::ParseError(format!(
                        "Ill-formed DesiredTransformValues: {e}"
                    )))
                })?.into_owned());
                in_value = false;
            }
            Event::End(e) => {
                let tag = local_tag(e.name().into_inner());
                if tag == "value" {
                    in_value = false;
                } else if tag == "transform" {
                    if let (Some(name), Some(value)) = (current_name.take(), current_value.take())
                    {
                        pairs.push((name, value));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(pairs)
}

/// Parses `SetStateVariables`'s `StateVariableValuePairs` argument: each
/// `<stateVariable variableName="...">`'s text content, in document order.
/// Structurally identical to [`parse_transform_settings`] (attribute name +
/// text content instead of attribute + child element), but kept separate
/// since the two dialects' tag/attribute names differ and conflating them
/// would need parser-level dialect switching for no real reuse.
fn parse_state_variable_value_pairs(xml: &str) -> Result<Vec<(String, String)>, SoapError> {
    let mut reader = quick_xml::reader::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut pairs = Vec::new();
    let mut current_name: Option<String> = None;
    let mut in_state_variable = false;

    loop {
        let event = reader.read_event().map_err(|e| {
            SoapError::Soap(SoapProtocolError::ParseError(format!(
                "Ill-formed StateVariableValuePairs: {e}"
            )))
        })?;
        match event {
            Event::Start(e) => {
                let tag = local_tag(e.name().into_inner());
                if tag == "stateVariable" {
                    current_name = e
                        .attributes()
                        .flatten()
                        .find(|a| local_tag(a.key.into_inner()) == "variableName")
                        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()));
                    in_state_variable = current_name.is_some();
                }
            }
            Event::Text(t) if in_state_variable => {
                if let Some(name) = current_name.take() {
                    let value = t.unescape().map_err(|e| {
                        SoapError::Soap(SoapProtocolError::ParseError(format!(
                            "Ill-formed StateVariableValuePairs: {e}"
                        )))
                    })?.into_owned();
                    pairs.push((name, value));
                }
                in_state_variable = false;
            }
            Event::End(e) => {
                if local_tag(e.name().into_inner()) == "stateVariable" {
                    in_state_variable = false;
                    current_name = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(pairs)
}

impl UPnPAction for RenderingControlAction {
    type Response = RenderingControlResponse;

    fn action_name(&self) -> &'static str {
        match self {
            Self::ListPresets(_) => "ListPresets",
            Self::SelectPreset(_) => "SelectPreset",
            Self::SetVolume(_) => "SetVolume",
            Self::SetMute(_) => "SetMute",
            Self::GetVolume(_) => "GetVolume",
            Self::GetMute(_) => "GetMute",
            Self::SetVolumeDB(_) => "SetVolumeDB",
            Self::GetVolumeDB(_) => "GetVolumeDB",
            Self::GetVolumeDBRange(_) => "GetVolumeDBRange",
            Self::SetBrightness(_) => "SetBrightness",
            Self::GetBrightness(_) => "GetBrightness",
            Self::SetContrast(_) => "SetContrast",
            Self::GetContrast(_) => "GetContrast",
            Self::SetRedVideoGain(_) => "SetRedVideoGain",
            Self::GetRedVideoGain(_) => "GetRedVideoGain",
            Self::SetGreenVideoGain(_) => "SetGreenVideoGain",
            Self::GetGreenVideoGain(_) => "GetGreenVideoGain",
            Self::SetBlueVideoGain(_) => "SetBlueVideoGain",
            Self::GetBlueVideoGain(_) => "GetBlueVideoGain",
            Self::SetRedVideoBlackLevel(_) => "SetRedVideoBlackLevel",
            Self::GetRedVideoBlackLevel(_) => "GetRedVideoBlackLevel",
            Self::SetGreenVideoBlackLevel(_) => "SetGreenVideoBlackLevel",
            Self::GetGreenVideoBlackLevel(_) => "GetGreenVideoBlackLevel",
            Self::SetBlueVideoBlackLevel(_) => "SetBlueVideoBlackLevel",
            Self::GetBlueVideoBlackLevel(_) => "GetBlueVideoBlackLevel",
            Self::SetColorTemperature(_) => "SetColorTemperature",
            Self::GetColorTemperature(_) => "GetColorTemperature",
            Self::SetHorizontalKeystone(_) => "SetHorizontalKeystone",
            Self::GetHorizontalKeystone(_) => "GetHorizontalKeystone",
            Self::SetVerticalKeystone(_) => "SetVerticalKeystone",
            Self::GetVerticalKeystone(_) => "GetVerticalKeystone",
            Self::SetSharpness(_) => "SetSharpness",
            Self::GetSharpness(_) => "GetSharpness",
            Self::SetLoudness(_) => "SetLoudness",
            Self::GetLoudness(_) => "GetLoudness",
            Self::GetStateVariables(_) => "GetStateVariables",
            Self::SetStateVariables(_) => "SetStateVariables",
            Self::GetAllowedTransforms(_) => "GetAllowedTransforms",
            Self::GetTransforms(_) => "GetTransforms",
            Self::SetTransforms(_) => "SetTransforms",
            Self::GetAllAvailableTransforms(_) => "GetAllAvailableTransforms",
        }
    }

    fn namespace(&self) -> &'static str {
        SERVICE_TYPE
    }

    async fn handle(&self, renderer: Arc<MediaRenderer>) -> Result<Self::Response, SoapError> {
        // Resolved once per request — see AVTransportAction::handle for why
        // this happens before any action-specific logic, including for
        // ListPresets/SelectPreset below, which don't otherwise touch
        // `control` but still carry an InstanceID a control point could get
        // wrong.
        let instance_id = self.instance_id();
        let control = renderer.rendering_control.get(instance_id)?;

        match self {
            // We only ever expose one preset — there's no persisted
            // configuration to snapshot/restore beyond current volume/mute —
            // but the actions themselves are required by the RenderingControl
            // spec, so we implement them rather than leaving them 401.
            Self::ListPresets(req) => {
                info!(instance_id = req.instance_id, "Handling ListPresets");
                Ok(RenderingControlResponse::ListPresets(ListPresetsResponse {
                    current_preset_name_list: "FactoryDefaults".to_string(),
                }))
            }
            Self::SelectPreset(req) => {
                if req.preset_name != "FactoryDefaults" {
                    warn!(
                        instance_id = req.instance_id,
                        preset_name = %req.preset_name,
                        "Handling SelectPreset"
                    );
                    return Err(SoapError::Soap(SoapProtocolError::InvalidPresetName(
                        format!("Unknown preset: {}", req.preset_name),
                    )));
                }
                info!(
                    instance_id = req.instance_id,
                    preset_name = %req.preset_name,
                    "Handling SelectPreset"
                );
                Ok(RenderingControlResponse::SelectPreset(
                    SelectPresetResponse {},
                ))
            }
            Self::SetVolume(req) => {
                validate_channel(&req.channel)?;
                let result = control.set_volume(req.desired_volume).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_volume = req.desired_volume,
                        "Handling SetVolume"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_volume = req.desired_volume,
                        error = %err,
                        "Handling SetVolume"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetVolume(SetVolumeResponse {}))
            }
            Self::SetMute(req) => {
                validate_channel(&req.channel)?;
                let result = control.set_mute(req.desired_mute).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_mute = req.desired_mute,
                        "Handling SetMute"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_mute = req.desired_mute,
                        error = %err,
                        "Handling SetMute"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetMute(SetMuteResponse {}))
            }
            Self::GetVolume(req) => {
                validate_channel(&req.channel)?;
                let result = control.get_volume().await;
                match &result {
                    Ok(current_volume) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        current_volume,
                        "Handling GetVolume"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        error = %err,
                        "Handling GetVolume"
                    ),
                }
                let current_volume = result?;
                Ok(RenderingControlResponse::Volume(GetVolumeResponse {
                    current_volume,
                }))
            }
            Self::GetMute(req) => {
                validate_channel(&req.channel)?;
                let result = control.get_mute().await;
                match &result {
                    Ok(current_mute) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        current_mute,
                        "Handling GetMute"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        error = %err,
                        "Handling GetMute"
                    ),
                }
                let current_mute = result?;
                Ok(RenderingControlResponse::Mute(GetMuteResponse {
                    current_mute,
                }))
            }
            Self::SetVolumeDB(req) => {
                validate_channel(&req.channel)?;
                let result = control.set_volume_db(req.desired_volume).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_volume = req.desired_volume,
                        "Handling SetVolumeDB"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_volume = req.desired_volume,
                        error = %err,
                        "Handling SetVolumeDB"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetVolumeDB(SetVolumeDBResponse {}))
            }
            Self::GetVolumeDB(req) => {
                validate_channel(&req.channel)?;
                let result = control.get_volume_db().await;
                match &result {
                    Ok(current_volume) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        current_volume,
                        "Handling GetVolumeDB"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        error = %err,
                        "Handling GetVolumeDB"
                    ),
                }
                let current_volume = result?;
                Ok(RenderingControlResponse::VolumeDB(GetVolumeDBResponse {
                    current_volume,
                }))
            }
            Self::GetVolumeDBRange(req) => {
                validate_channel(&req.channel)?;
                let (min_value, max_value) = control.volume_db_range();
                info!(
                    instance_id = req.instance_id,
                    channel = %req.channel,
                    min_value,
                    max_value,
                    "Handling GetVolumeDBRange"
                );
                Ok(RenderingControlResponse::VolumeDBRange(
                    GetVolumeDBRangeResponse {
                        min_value,
                        max_value,
                    },
                ))
            }
            Self::SetBrightness(req) => {
                let result = control.set_brightness(req.desired_brightness).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_brightness = req.desired_brightness,
                        "Handling SetBrightness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_brightness = req.desired_brightness,
                        error = %err,
                        "Handling SetBrightness"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetBrightness(
                    SetBrightnessResponse {},
                ))
            }
            Self::GetBrightness(req) => {
                let result = control.get_brightness().await;
                match &result {
                    Ok(current_brightness) => info!(
                        instance_id = req.instance_id,
                        current_brightness,
                        "Handling GetBrightness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetBrightness"
                    ),
                }
                let current_brightness = result?;
                Ok(RenderingControlResponse::Brightness(
                    GetBrightnessResponse { current_brightness },
                ))
            }
            Self::SetContrast(req) => {
                let result = control.set_contrast(req.desired_contrast).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_contrast = req.desired_contrast,
                        "Handling SetContrast"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_contrast = req.desired_contrast,
                        error = %err,
                        "Handling SetContrast"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetContrast(
                    SetContrastResponse {},
                ))
            }
            Self::GetContrast(req) => {
                let result = control.get_contrast().await;
                match &result {
                    Ok(current_contrast) => info!(
                        instance_id = req.instance_id,
                        current_contrast,
                        "Handling GetContrast"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetContrast"
                    ),
                }
                let current_contrast = result?;
                Ok(RenderingControlResponse::Contrast(GetContrastResponse {
                    current_contrast,
                }))
            }
            Self::SetRedVideoGain(req) => {
                let result = control.set_red_video_gain(req.desired_red_video_gain).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_red_video_gain = req.desired_red_video_gain,
                        "Handling SetRedVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_red_video_gain = req.desired_red_video_gain,
                        error = %err,
                        "Handling SetRedVideoGain"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetRedVideoGain(
                    SetRedVideoGainResponse {},
                ))
            }
            Self::GetRedVideoGain(req) => {
                let result = control.get_red_video_gain().await;
                match &result {
                    Ok(current_red_video_gain) => info!(
                        instance_id = req.instance_id,
                        current_red_video_gain,
                        "Handling GetRedVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetRedVideoGain"
                    ),
                }
                let current_red_video_gain = result?;
                Ok(RenderingControlResponse::RedVideoGain(
                    GetRedVideoGainResponse {
                        current_red_video_gain,
                    },
                ))
            }
            Self::SetGreenVideoGain(req) => {
                let result = control
                    .set_green_video_gain(req.desired_green_video_gain)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_green_video_gain = req.desired_green_video_gain,
                        "Handling SetGreenVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_green_video_gain = req.desired_green_video_gain,
                        error = %err,
                        "Handling SetGreenVideoGain"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetGreenVideoGain(
                    SetGreenVideoGainResponse {},
                ))
            }
            Self::GetGreenVideoGain(req) => {
                let result = control.get_green_video_gain().await;
                match &result {
                    Ok(current_green_video_gain) => info!(
                        instance_id = req.instance_id,
                        current_green_video_gain,
                        "Handling GetGreenVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetGreenVideoGain"
                    ),
                }
                let current_green_video_gain = result?;
                Ok(RenderingControlResponse::GreenVideoGain(
                    GetGreenVideoGainResponse {
                        current_green_video_gain,
                    },
                ))
            }
            Self::SetBlueVideoGain(req) => {
                let result = control.set_blue_video_gain(req.desired_blue_video_gain).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_blue_video_gain = req.desired_blue_video_gain,
                        "Handling SetBlueVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_blue_video_gain = req.desired_blue_video_gain,
                        error = %err,
                        "Handling SetBlueVideoGain"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetBlueVideoGain(
                    SetBlueVideoGainResponse {},
                ))
            }
            Self::GetBlueVideoGain(req) => {
                let result = control.get_blue_video_gain().await;
                match &result {
                    Ok(current_blue_video_gain) => info!(
                        instance_id = req.instance_id,
                        current_blue_video_gain,
                        "Handling GetBlueVideoGain"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetBlueVideoGain"
                    ),
                }
                let current_blue_video_gain = result?;
                Ok(RenderingControlResponse::BlueVideoGain(
                    GetBlueVideoGainResponse {
                        current_blue_video_gain,
                    },
                ))
            }
            Self::SetRedVideoBlackLevel(req) => {
                let result = control
                    .set_red_video_black_level(req.desired_red_video_black_level)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_red_video_black_level = req.desired_red_video_black_level,
                        "Handling SetRedVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_red_video_black_level = req.desired_red_video_black_level,
                        error = %err,
                        "Handling SetRedVideoBlackLevel"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetRedVideoBlackLevel(
                    SetRedVideoBlackLevelResponse {},
                ))
            }
            Self::GetRedVideoBlackLevel(req) => {
                let result = control.get_red_video_black_level().await;
                match &result {
                    Ok(current_red_video_black_level) => info!(
                        instance_id = req.instance_id,
                        current_red_video_black_level,
                        "Handling GetRedVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetRedVideoBlackLevel"
                    ),
                }
                let current_red_video_black_level = result?;
                Ok(RenderingControlResponse::RedVideoBlackLevel(
                    GetRedVideoBlackLevelResponse {
                        current_red_video_black_level,
                    },
                ))
            }
            Self::SetGreenVideoBlackLevel(req) => {
                let result = control
                    .set_green_video_black_level(req.desired_green_video_black_level)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_green_video_black_level = req.desired_green_video_black_level,
                        "Handling SetGreenVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_green_video_black_level = req.desired_green_video_black_level,
                        error = %err,
                        "Handling SetGreenVideoBlackLevel"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetGreenVideoBlackLevel(
                    SetGreenVideoBlackLevelResponse {},
                ))
            }
            Self::GetGreenVideoBlackLevel(req) => {
                let result = control.get_green_video_black_level().await;
                match &result {
                    Ok(current_green_video_black_level) => info!(
                        instance_id = req.instance_id,
                        current_green_video_black_level,
                        "Handling GetGreenVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetGreenVideoBlackLevel"
                    ),
                }
                let current_green_video_black_level = result?;
                Ok(RenderingControlResponse::GreenVideoBlackLevel(
                    GetGreenVideoBlackLevelResponse {
                        current_green_video_black_level,
                    },
                ))
            }
            Self::SetBlueVideoBlackLevel(req) => {
                let result = control
                    .set_blue_video_black_level(req.desired_blue_video_black_level)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_blue_video_black_level = req.desired_blue_video_black_level,
                        "Handling SetBlueVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_blue_video_black_level = req.desired_blue_video_black_level,
                        error = %err,
                        "Handling SetBlueVideoBlackLevel"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetBlueVideoBlackLevel(
                    SetBlueVideoBlackLevelResponse {},
                ))
            }
            Self::GetBlueVideoBlackLevel(req) => {
                let result = control.get_blue_video_black_level().await;
                match &result {
                    Ok(current_blue_video_black_level) => info!(
                        instance_id = req.instance_id,
                        current_blue_video_black_level,
                        "Handling GetBlueVideoBlackLevel"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetBlueVideoBlackLevel"
                    ),
                }
                let current_blue_video_black_level = result?;
                Ok(RenderingControlResponse::BlueVideoBlackLevel(
                    GetBlueVideoBlackLevelResponse {
                        current_blue_video_black_level,
                    },
                ))
            }
            Self::SetColorTemperature(req) => {
                let result = control
                    .set_color_temperature(req.desired_color_temperature)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_color_temperature = req.desired_color_temperature,
                        "Handling SetColorTemperature"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_color_temperature = req.desired_color_temperature,
                        error = %err,
                        "Handling SetColorTemperature"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetColorTemperature(
                    SetColorTemperatureResponse {},
                ))
            }
            Self::GetColorTemperature(req) => {
                let result = control.get_color_temperature().await;
                match &result {
                    Ok(current_color_temperature) => info!(
                        instance_id = req.instance_id,
                        current_color_temperature,
                        "Handling GetColorTemperature"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetColorTemperature"
                    ),
                }
                let current_color_temperature = result?;
                Ok(RenderingControlResponse::ColorTemperature(
                    GetColorTemperatureResponse {
                        current_color_temperature,
                    },
                ))
            }
            Self::SetHorizontalKeystone(req) => {
                let result = control
                    .set_horizontal_keystone(req.desired_horizontal_keystone)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_horizontal_keystone = req.desired_horizontal_keystone,
                        "Handling SetHorizontalKeystone"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_horizontal_keystone = req.desired_horizontal_keystone,
                        error = %err,
                        "Handling SetHorizontalKeystone"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetHorizontalKeystone(
                    SetHorizontalKeystoneResponse {},
                ))
            }
            Self::GetHorizontalKeystone(req) => {
                let result = control.get_horizontal_keystone().await;
                match &result {
                    Ok(current_horizontal_keystone) => info!(
                        instance_id = req.instance_id,
                        current_horizontal_keystone,
                        "Handling GetHorizontalKeystone"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetHorizontalKeystone"
                    ),
                }
                let current_horizontal_keystone = result?;
                Ok(RenderingControlResponse::HorizontalKeystone(
                    GetHorizontalKeystoneResponse {
                        current_horizontal_keystone,
                    },
                ))
            }
            Self::SetVerticalKeystone(req) => {
                let result = control
                    .set_vertical_keystone(req.desired_vertical_keystone)
                    .await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_vertical_keystone = req.desired_vertical_keystone,
                        "Handling SetVerticalKeystone"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_vertical_keystone = req.desired_vertical_keystone,
                        error = %err,
                        "Handling SetVerticalKeystone"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetVerticalKeystone(
                    SetVerticalKeystoneResponse {},
                ))
            }
            Self::GetVerticalKeystone(req) => {
                let result = control.get_vertical_keystone().await;
                match &result {
                    Ok(current_vertical_keystone) => info!(
                        instance_id = req.instance_id,
                        current_vertical_keystone,
                        "Handling GetVerticalKeystone"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetVerticalKeystone"
                    ),
                }
                let current_vertical_keystone = result?;
                Ok(RenderingControlResponse::VerticalKeystone(
                    GetVerticalKeystoneResponse {
                        current_vertical_keystone,
                    },
                ))
            }
            Self::SetSharpness(req) => {
                let result = control.set_sharpness(req.desired_sharpness).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        desired_sharpness = req.desired_sharpness,
                        "Handling SetSharpness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        desired_sharpness = req.desired_sharpness,
                        error = %err,
                        "Handling SetSharpness"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetSharpness(
                    SetSharpnessResponse {},
                ))
            }
            Self::GetSharpness(req) => {
                let result = control.get_sharpness().await;
                match &result {
                    Ok(current_sharpness) => info!(
                        instance_id = req.instance_id,
                        current_sharpness,
                        "Handling GetSharpness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        error = %err,
                        "Handling GetSharpness"
                    ),
                }
                let current_sharpness = result?;
                Ok(RenderingControlResponse::Sharpness(GetSharpnessResponse {
                    current_sharpness,
                }))
            }
            Self::SetLoudness(req) => {
                validate_channel(&req.channel)?;
                let result = control.set_loudness(req.desired_loudness).await;
                match &result {
                    Ok(_) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_loudness = req.desired_loudness,
                        "Handling SetLoudness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        desired_loudness = req.desired_loudness,
                        error = %err,
                        "Handling SetLoudness"
                    ),
                }
                result?;
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetLoudness(
                    SetLoudnessResponse {},
                ))
            }
            Self::GetLoudness(req) => {
                validate_channel(&req.channel)?;
                let result = control.get_loudness().await;
                match &result {
                    Ok(current_loudness) => info!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        current_loudness,
                        "Handling GetLoudness"
                    ),
                    Err(err) => warn!(
                        instance_id = req.instance_id,
                        channel = %req.channel,
                        error = %err,
                        "Handling GetLoudness"
                    ),
                }
                let current_loudness = result?;
                Ok(RenderingControlResponse::Loudness(GetLoudnessResponse {
                    current_loudness,
                }))
            }
            Self::GetStateVariables(req) => {
                let names: Vec<&str> = if req.state_variable_list.trim() == "*" {
                    ALL_STATE_VARIABLE_NAMES.to_vec()
                } else {
                    req.state_variable_list
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect()
                };
                if names.is_empty() {
                    warn!(
                        instance_id = req.instance_id,
                        state_variable_list = %req.state_variable_list,
                        "Handling GetStateVariables"
                    );
                    return Err(SoapError::Soap(SoapProtocolError::InvalidStateVariableList(
                        req.state_variable_list.clone(),
                    )));
                }

                let mut pairs = String::new();
                for name in &names {
                    let value = match state_variable_value(control.as_ref(), name).await {
                        Some(Ok(v)) => v,
                        Some(Err(err)) => return Err(err),
                        None => {
                            warn!(
                                instance_id = req.instance_id,
                                variable_name = %name,
                                "Handling GetStateVariables"
                            );
                            return Err(SoapError::Soap(SoapProtocolError::InvalidStateVariableList(
                                (*name).to_string(),
                            )));
                        }
                    };
                    let (value, has_channel) = value;
                    if has_channel {
                        pairs.push_str(&format!(
                            r#"<stateVariable variableName="{name}" channel="Master">{value}</stateVariable>"#
                        ));
                    } else {
                        pairs.push_str(&format!(
                            r#"<stateVariable variableName="{name}">{value}</stateVariable>"#
                        ));
                    }
                }

                info!(
                    instance_id = req.instance_id,
                    state_variable_list = %req.state_variable_list,
                    "Handling GetStateVariables"
                );
                let state_variable_value_pairs = format!(
                    concat!(
                        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
                        r#"<stateVariableValuePairs xmlns="urn:schemas-upnp-org:av:avs">"#,
                        "{pairs}",
                        r#"</stateVariableValuePairs>"#
                    ),
                    pairs = pairs,
                );
                Ok(RenderingControlResponse::StateVariables(
                    GetStateVariablesResponse {
                        state_variable_value_pairs,
                    },
                ))
            }
            Self::SetStateVariables(req) => {
                let expected_udn = format!("uuid:{}", renderer.unique_id());
                if req.rendering_control_udn != expected_udn {
                    warn!(
                        instance_id = req.instance_id,
                        rendering_control_udn = %req.rendering_control_udn,
                        "Handling SetStateVariables"
                    );
                    return Err(SoapError::Soap(SoapProtocolError::InvalidRenderingControlUdn(
                        req.rendering_control_udn.clone(),
                    )));
                }
                if req.service_type != SERVICE_TYPE {
                    warn!(
                        instance_id = req.instance_id,
                        service_type = %req.service_type,
                        "Handling SetStateVariables"
                    );
                    return Err(SoapError::Soap(SoapProtocolError::InvalidServiceType(
                        req.service_type.clone(),
                    )));
                }
                if req.service_id != SERVICE_ID {
                    warn!(
                        instance_id = req.instance_id,
                        service_id = %req.service_id,
                        "Handling SetStateVariables"
                    );
                    return Err(SoapError::Soap(SoapProtocolError::InvalidServiceId(
                        req.service_id.clone(),
                    )));
                }

                let pairs = parse_state_variable_value_pairs(&req.state_variable_value_pairs)?;

                // Validated up front, all-or-nothing - unlike SetTransforms,
                // nothing here says SetStateVariables tolerates a partial
                // application, and 5.4.38's own wording ("all state
                // variable values will be replaced") reads as all-or-
                // nothing once the UDN/ServiceType/ServiceId check above
                // passes.
                for (name, _) in &pairs {
                    if !ALL_STATE_VARIABLE_NAMES.contains(&name.as_str()) {
                        warn!(
                            instance_id = req.instance_id,
                            variable_name = %name,
                            "Handling SetStateVariables"
                        );
                        return Err(SoapError::Soap(
                            SoapProtocolError::StateVariablesSpecifiedImproperly(name.clone()),
                        ));
                    }
                }

                for (name, value) in &pairs {
                    let transform_name = match name.as_str() {
                        "Volume" => "Volume_Master",
                        "VolumeDB" => "VolumeDB_Master",
                        "Mute" => "Mute_Master",
                        "Loudness" => "Loudness_Master",
                        other => other,
                    };
                    if apply_transform(control.as_ref(), transform_name, value)
                        .await
                        .is_err()
                    {
                        warn!(
                            instance_id = req.instance_id,
                            variable_name = %name,
                            value = %value,
                            "Handling SetStateVariables"
                        );
                        return Err(SoapError::Soap(SoapProtocolError::InvalidStateVariableValue(
                            name.clone(),
                        )));
                    }
                }

                let state_variable_list =
                    pairs.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(",");
                info!(
                    instance_id = req.instance_id,
                    state_variable_list = %state_variable_list,
                    "Handling SetStateVariables"
                );
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetStateVariables(
                    SetStateVariablesResponse { state_variable_list },
                ))
            }
            Self::GetAllowedTransforms(req) => {
                info!(instance_id = req.instance_id, "Handling GetAllowedTransforms");
                Ok(RenderingControlResponse::AllowedTransforms(
                    GetAllowedTransformsResponse {
                        current_allowed_transform_settings: allowed_transform_settings_xml(),
                    },
                ))
            }
            Self::GetTransforms(req) => {
                let current_transform_values =
                    current_transform_settings_xml(control.as_ref()).await?;
                info!(instance_id = req.instance_id, "Handling GetTransforms");
                Ok(RenderingControlResponse::Transforms(GetTransformsResponse {
                    current_transform_values,
                }))
            }
            Self::SetTransforms(req) => {
                let pairs = parse_transform_settings(&req.desired_transform_values)?;
                if pairs.is_empty() {
                    warn!(instance_id = req.instance_id, "Handling SetTransforms");
                    return Err(SoapError::Soap(SoapProtocolError::TransformsNotAllowed(
                        "DesiredTransformValues named no transforms".to_string(),
                    )));
                }

                let mut applied = 0usize;
                let mut unknown_transform: Option<String> = None;
                let mut unsupported_value: Option<String> = None;
                for (name, value) in &pairs {
                    match apply_transform(control.as_ref(), name, value).await {
                        Ok(()) => applied += 1,
                        Err(TransformApplyError::UnknownTransform) => {
                            unknown_transform.get_or_insert_with(|| name.clone());
                        }
                        Err(TransformApplyError::UnsupportedValue) => {
                            unsupported_value.get_or_insert_with(|| name.clone());
                        }
                    }
                }

                // Per 5.4.41: the action succeeds if *any* requested
                // transform succeeds - the rest silently keep their prior
                // value, discoverable only via the LastChange NOTIFY this
                // still publishes below, not via this response.
                if applied == 0 {
                    warn!(
                        instance_id = req.instance_id,
                        requested = pairs.len(),
                        "Handling SetTransforms"
                    );
                    if let Some(name) = unknown_transform {
                        return Err(SoapError::Soap(SoapProtocolError::TransformsNotAllowed(
                            name,
                        )));
                    }
                    return Err(SoapError::Soap(SoapProtocolError::UnsupportedTransformValue(
                        unsupported_value.unwrap_or_default(),
                    )));
                }

                info!(
                    instance_id = req.instance_id,
                    applied,
                    requested = pairs.len(),
                    "Handling SetTransforms"
                );
                publish_rendering_state(instance_id, control.as_ref(), &renderer.rendering_control_events)
                    .await;
                Ok(RenderingControlResponse::SetTransforms(
                    SetTransformsResponse {},
                ))
            }
            Self::GetAllAvailableTransforms(_) => {
                info!("Handling GetAllAvailableTransforms");
                Ok(RenderingControlResponse::AllAvailableTransforms(
                    GetAllAvailableTransformsResponse {
                        all_allowed_transform_settings: allowed_transform_settings_xml(),
                    },
                ))
            }
        }
    }
}
