//! A typed RenderingControl control point - method names mirror
//! `upnp-suite-renderer`'s own `RenderingControl` service trait.

use reqwest::Url;

use crate::error::ClientError;
use crate::soap::invoke_action;

const DEFAULT_INSTANCE_ID: &str = "0";
const MASTER_CHANNEL: &str = "Master";

/// What `GetVolumeDBRange` reports - the device's supported volume range
/// in hundredths of a decibel, as opposed to `GetVolume`'s 0-100 scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VolumeDbRange {
    pub min: i16,
    pub max: i16,
}

pub struct RenderingControlClient<'a> {
    http: &'a reqwest::Client,
    control_url: &'a Url,
    service_type: &'a str,
}

impl<'a> RenderingControlClient<'a> {
    pub(crate) fn new(
        http: &'a reqwest::Client,
        control_url: &'a Url,
        service_type: &'a str,
    ) -> Self {
        Self {
            http,
            control_url,
            service_type,
        }
    }

    async fn invoke(
        &self,
        action: &str,
        args: &[(&str, &str)],
    ) -> Result<std::collections::HashMap<String, String>, ClientError> {
        invoke_action(self.http, self.control_url, self.service_type, action, args).await
    }

    pub async fn get_volume(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke(
                "GetVolume",
                &[
                    ("InstanceID", DEFAULT_INSTANCE_ID),
                    ("Channel", MASTER_CHANNEL),
                ],
            )
            .await?;

        fields
            .get("CurrentVolume")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| ClientError::MalformedResponse("response missing CurrentVolume".into()))
    }

    pub async fn set_volume(&self, volume: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetVolume",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("Channel", MASTER_CHANNEL),
                ("DesiredVolume", &volume.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_mute(&self) -> Result<bool, ClientError> {
        let fields = self
            .invoke(
                "GetMute",
                &[
                    ("InstanceID", DEFAULT_INSTANCE_ID),
                    ("Channel", MASTER_CHANNEL),
                ],
            )
            .await?;

        // UPnP's boolean encoding is nominally "0"/"1", but "true"/"false"
        // shows up in the wild too (including from this project's own
        // renderer) - accept either rather than assuming one.
        Ok(fields
            .get("CurrentMute")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false))
    }

    pub async fn set_mute(&self, muted: bool) -> Result<(), ClientError> {
        self.invoke(
            "SetMute",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("Channel", MASTER_CHANNEL),
                ("DesiredMute", if muted { "1" } else { "0" }),
            ],
        )
        .await?;
        Ok(())
    }

    /// UPnP's `Brightness`/`Contrast` - 0-100, 50 = neutral/no adjustment.
    /// No `Channel` argument, unlike `GetVolume`/`GetMute` above - these are
    /// a display setting, not per audio-channel (see the RenderingControl
    /// spec's argument tables for `GetBrightness`/`SetBrightness`).
    pub async fn get_brightness(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke("GetBrightness", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;

        fields
            .get("CurrentBrightness")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse("response missing CurrentBrightness".into())
            })
    }

    pub async fn set_brightness(&self, brightness: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetBrightness",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredBrightness", &brightness.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_contrast(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke("GetContrast", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;

        fields
            .get("CurrentContrast")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse("response missing CurrentContrast".into())
            })
    }

    pub async fn set_contrast(&self, contrast: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetContrast",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredContrast", &contrast.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    /// UPnP's 2-point white balance pair per primary color -
    /// `Output = Gain * Input + BlackLevel`. 0-1000, 500 = neutral for
    /// both. No `Channel` argument, same reasoning as `Brightness`/`Contrast`.
    pub async fn get_red_video_gain(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke("GetRedVideoGain", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        fields
            .get("CurrentRedVideoGain")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse("response missing CurrentRedVideoGain".into())
            })
    }

    pub async fn set_red_video_gain(&self, gain: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetRedVideoGain",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredRedVideoGain", &gain.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_green_video_gain(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke("GetGreenVideoGain", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        fields
            .get("CurrentGreenVideoGain")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse("response missing CurrentGreenVideoGain".into())
            })
    }

    pub async fn set_green_video_gain(&self, gain: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetGreenVideoGain",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredGreenVideoGain", &gain.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_blue_video_gain(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke("GetBlueVideoGain", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        fields
            .get("CurrentBlueVideoGain")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse("response missing CurrentBlueVideoGain".into())
            })
    }

    pub async fn set_blue_video_gain(&self, gain: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetBlueVideoGain",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredBlueVideoGain", &gain.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_red_video_black_level(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke(
                "GetRedVideoBlackLevel",
                &[("InstanceID", DEFAULT_INSTANCE_ID)],
            )
            .await?;
        fields
            .get("CurrentRedVideoBlackLevel")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse(
                    "response missing CurrentRedVideoBlackLevel".into(),
                )
            })
    }

    pub async fn set_red_video_black_level(&self, level: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetRedVideoBlackLevel",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredRedVideoBlackLevel", &level.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_green_video_black_level(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke(
                "GetGreenVideoBlackLevel",
                &[("InstanceID", DEFAULT_INSTANCE_ID)],
            )
            .await?;
        fields
            .get("CurrentGreenVideoBlackLevel")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse(
                    "response missing CurrentGreenVideoBlackLevel".into(),
                )
            })
    }

    pub async fn set_green_video_black_level(&self, level: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetGreenVideoBlackLevel",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredGreenVideoBlackLevel", &level.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_blue_video_black_level(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke(
                "GetBlueVideoBlackLevel",
                &[("InstanceID", DEFAULT_INSTANCE_ID)],
            )
            .await?;
        fields
            .get("CurrentBlueVideoBlackLevel")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse(
                    "response missing CurrentBlueVideoBlackLevel".into(),
                )
            })
    }

    pub async fn set_blue_video_black_level(&self, level: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetBlueVideoBlackLevel",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredBlueVideoBlackLevel", &level.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    /// UPnP's `ColorTemperature` - 0-100, 50 = neutral = D65 (6500K).
    pub async fn get_color_temperature(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke("GetColorTemperature", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        fields
            .get("CurrentColorTemperature")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse("response missing CurrentColorTemperature".into())
            })
    }

    pub async fn set_color_temperature(&self, temperature: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetColorTemperature",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredColorTemperature", &temperature.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    /// UPnP's `HorizontalKeystone`/`VerticalKeystone` - -50..=50, 0 =
    /// neutral. A trapezoidal warp of the displayed video's shape (real
    /// projector keystone correction), not a color adjustment. No
    /// `Channel` argument, same reasoning as `Brightness`/`Contrast`.
    pub async fn get_horizontal_keystone(&self) -> Result<i16, ClientError> {
        let fields = self
            .invoke("GetHorizontalKeystone", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        fields
            .get("CurrentHorizontalKeystone")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse(
                    "response missing CurrentHorizontalKeystone".into(),
                )
            })
    }

    pub async fn set_horizontal_keystone(&self, keystone: i16) -> Result<(), ClientError> {
        self.invoke(
            "SetHorizontalKeystone",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredHorizontalKeystone", &keystone.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_vertical_keystone(&self) -> Result<i16, ClientError> {
        let fields = self
            .invoke("GetVerticalKeystone", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        fields
            .get("CurrentVerticalKeystone")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| {
                ClientError::MalformedResponse("response missing CurrentVerticalKeystone".into())
            })
    }

    pub async fn set_vertical_keystone(&self, keystone: i16) -> Result<(), ClientError> {
        self.invoke(
            "SetVerticalKeystone",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredVerticalKeystone", &keystone.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    /// UPnP's `Sharpness` - 0-100, 0 = neutral. Unlike every other picture
    /// control here, neutral sits at the range's own minimum, not its
    /// midpoint - there's no "soften" concept, only "how much sharpening".
    pub async fn get_sharpness(&self) -> Result<u16, ClientError> {
        let fields = self
            .invoke("GetSharpness", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        fields
            .get("CurrentSharpness")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| ClientError::MalformedResponse("response missing CurrentSharpness".into()))
    }

    pub async fn set_sharpness(&self, sharpness: u16) -> Result<(), ClientError> {
        self.invoke(
            "SetSharpness",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("DesiredSharpness", &sharpness.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_volume_db_range(&self) -> Result<VolumeDbRange, ClientError> {
        let fields = self
            .invoke(
                "GetVolumeDBRange",
                &[
                    ("InstanceID", DEFAULT_INSTANCE_ID),
                    ("Channel", MASTER_CHANNEL),
                ],
            )
            .await?;

        Ok(VolumeDbRange {
            min: fields
                .get("MinValue")
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| ClientError::MalformedResponse("response missing MinValue".into()))?,
            max: fields
                .get("MaxValue")
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| ClientError::MalformedResponse("response missing MaxValue".into()))?,
        })
    }

    /// The comma-separated list of preset names the device supports (at
    /// minimum `FactoryDefaults`, per spec), split into individual names.
    pub async fn list_presets(&self) -> Result<Vec<String>, ClientError> {
        let fields = self
            .invoke("ListPresets", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;

        let list = fields.get("CurrentPresetNameList").cloned().unwrap_or_default();
        Ok(list
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect())
    }
}
