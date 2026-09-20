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
}

fn field(fields: &std::collections::HashMap<String, String>, name: &str) -> Result<String, ClientError> {
    fields
        .get(name)
        .cloned()
        .ok_or_else(|| ClientError::MalformedResponse(format!("response missing {name}")))
}
