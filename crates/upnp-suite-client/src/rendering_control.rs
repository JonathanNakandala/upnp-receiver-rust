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
