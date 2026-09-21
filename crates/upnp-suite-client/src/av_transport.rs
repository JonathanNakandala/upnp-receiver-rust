//! A typed AVTransport control point — method names/signatures mirror
//! `upnp-suite-renderer`'s own `AVTransport` service trait, so a caller
//! driving playback over SOAP reads the same as one driving the trait
//! object in-process would.

use reqwest::Url;

use crate::error::ClientError;
use crate::soap::invoke_action;

const DEFAULT_INSTANCE_ID: &str = "0";

/// What `GetTransportInfo` reports.
#[derive(Debug, Clone, PartialEq)]
pub struct TransportInfo {
    pub current_transport_state: String,
    pub current_transport_status: String,
    /// The raw `TransportPlaySpeed` string (e.g. `"1"`, `"1/2"`) - not
    /// parsed to `f64` here since this crate has no opinion on which
    /// speeds a given renderer actually supports.
    pub current_speed: String,
}

/// What `GetPositionInfo` reports - track position isn't GENA-evented (no
/// `LastChange` variable carries it), so this is the only way a control
/// point learns it, typically by polling.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PositionInfo {
    pub track: String,
    /// `H+:MM:SS` (or `NOT_IMPLEMENTED`), same raw format the device sends.
    pub track_duration: String,
    pub track_uri: String,
    pub track_metadata: Option<String>,
    /// `H+:MM:SS` elapsed within the current track.
    pub rel_time: String,
    /// `H+:MM:SS` elapsed since the start of the AVTransportURI resource.
    pub abs_time: String,
}

/// What `GetMediaInfo` reports - the currently/next loaded resource plus
/// the storage media involved, as opposed to `GetPositionInfo`'s playback
/// progress within it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaInfo {
    pub nr_tracks: u32,
    pub media_duration: String,
    pub current_uri: String,
    pub current_uri_metadata: Option<String>,
    pub next_uri: String,
    pub next_uri_metadata: Option<String>,
    pub play_medium: String,
    pub record_medium: String,
    pub write_status: String,
}

/// What `GetTransportSettings` reports - the instance's current play/
/// record quality mode, as opposed to `GetDeviceCapabilities`' list of
/// modes the device supports at all.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransportSettings {
    pub play_mode: String,
    pub rec_quality_mode: String,
}

/// What `GetDeviceCapabilities` reports - each field is a comma-separated
/// list of the media/quality-mode values the device supports, same raw
/// format the device sends (never parsed further here, same as this
/// crate's other comma-separated capability lists).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeviceCapabilities {
    pub play_media: String,
    pub rec_media: String,
    pub rec_quality_modes: String,
}

pub struct AVTransportClient<'a> {
    http: &'a reqwest::Client,
    control_url: &'a Url,
    service_type: &'a str,
}

impl<'a> AVTransportClient<'a> {
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

    pub async fn play(&self) -> Result<(), ClientError> {
        self.invoke(
            "Play",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("Speed", "1"),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<(), ClientError> {
        self.invoke("Pause", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), ClientError> {
        self.invoke("Stop", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;
        Ok(())
    }

    /// Changes playback rate - UPnP has no separate "set speed" action;
    /// this renderer (and the spec) folds it into `Play`'s `Speed`
    /// argument, so this reissues `Play` at the new rate.
    pub async fn set_play_speed(&self, speed: f64) -> Result<(), ClientError> {
        self.invoke(
            "Play",
            &[
                ("InstanceID", DEFAULT_INSTANCE_ID),
                ("Speed", &speed.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn get_transport_info(&self) -> Result<TransportInfo, ClientError> {
        let fields = self
            .invoke("GetTransportInfo", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;

        Ok(TransportInfo {
            current_transport_state: field(&fields, "CurrentTransportState")?,
            current_transport_status: field(&fields, "CurrentTransportStatus")?,
            current_speed: field(&fields, "CurrentSpeed")?,
        })
    }

    pub async fn get_position_info(&self) -> Result<PositionInfo, ClientError> {
        let fields = self
            .invoke("GetPositionInfo", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;

        Ok(PositionInfo {
            track: field(&fields, "Track")?,
            track_duration: field(&fields, "TrackDuration")?,
            track_uri: fields.get("TrackURI").cloned().unwrap_or_default(),
            track_metadata: fields
                .get("TrackMetaData")
                .cloned()
                .filter(|s| !s.is_empty()),
            rel_time: field(&fields, "RelTime")?,
            abs_time: fields.get("AbsTime").cloned().unwrap_or_default(),
        })
    }

    pub async fn get_media_info(&self) -> Result<MediaInfo, ClientError> {
        let fields = self
            .invoke("GetMediaInfo", &[("InstanceID", DEFAULT_INSTANCE_ID)])
            .await?;

        Ok(MediaInfo {
            nr_tracks: field(&fields, "NrTracks")?.parse().unwrap_or_default(),
            media_duration: field(&fields, "MediaDuration")?,
            current_uri: field(&fields, "CurrentURI")?,
            current_uri_metadata: fields
                .get("CurrentURIMetaData")
                .cloned()
                .filter(|s| !s.is_empty()),
            next_uri: fields.get("NextURI").cloned().unwrap_or_default(),
            next_uri_metadata: fields
                .get("NextURIMetaData")
                .cloned()
                .filter(|s| !s.is_empty()),
            play_medium: field(&fields, "PlayMedium")?,
            record_medium: fields.get("RecordMedium").cloned().unwrap_or_default(),
            write_status: fields.get("WriteStatus").cloned().unwrap_or_default(),
        })
    }

    pub async fn get_transport_settings(&self) -> Result<TransportSettings, ClientError> {
        let fields = self
            .invoke(
                "GetTransportSettings",
                &[("InstanceID", DEFAULT_INSTANCE_ID)],
            )
            .await?;

        Ok(TransportSettings {
            play_mode: field(&fields, "PlayMode")?,
            rec_quality_mode: fields.get("RecQualityMode").cloned().unwrap_or_default(),
        })
    }

    /// The comma-separated list of actions currently valid to call given
    /// the transport's present state (e.g. `Play` absent while already
    /// playing) - split into individual action names.
    pub async fn get_current_transport_actions(&self) -> Result<Vec<String>, ClientError> {
        let fields = self
            .invoke(
                "GetCurrentTransportActions",
                &[("InstanceID", DEFAULT_INSTANCE_ID)],
            )
            .await?;

        Ok(split_csv(&field(&fields, "Actions")?))
    }

    /// What the device supports at all (not instance state) - which play/
    /// record media and record quality modes it's capable of.
    pub async fn get_device_capabilities(&self) -> Result<DeviceCapabilities, ClientError> {
        let fields = self
            .invoke(
                "GetDeviceCapabilities",
                &[("InstanceID", DEFAULT_INSTANCE_ID)],
            )
            .await?;

        Ok(DeviceCapabilities {
            play_media: field(&fields, "PlayMedia")?,
            rec_media: fields.get("RecMedia").cloned().unwrap_or_default(),
            rec_quality_modes: fields.get("RecQualityModes").cloned().unwrap_or_default(),
        })
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn field(fields: &std::collections::HashMap<String, String>, name: &str) -> Result<String, ClientError> {
    fields
        .get(name)
        .cloned()
        .ok_or_else(|| ClientError::MalformedResponse(format!("response missing {name}")))
}
