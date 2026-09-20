//! Resolves a device description URL into control points for the services
//! this crate knows how to drive — the entry point every caller (the video
//! window's menu, `upnp-suite-cli`) starts from.

use std::sync::Arc;

use reqwest::Url;
use tokio::sync::OnceCell;
use upnp_suite_core::discovery::description::{fetch_device_report, ServiceReport};

use crate::av_transport::AVTransportClient;
use crate::error::ClientError;
use crate::eventing::NotifyListener;
use crate::rendering_control::RenderingControlClient;

const AV_TRANSPORT_PREFIX: &str = "urn:schemas-upnp-org:service:AVTransport:";
const RENDERING_CONTROL_PREFIX: &str = "urn:schemas-upnp-org:service:RenderingControl:";

/// A resolved connection to one UPnP MediaRenderer's control endpoints.
/// Cheap to construct multiple of against the same device (each just holds
/// a `reqwest::Client` and a couple of resolved URLs) - callers wanting a
/// single shared connection should hold on to one instance rather than
/// reconnecting per action.
pub struct RendererClient {
    pub(crate) http: reqwest::Client,
    pub(crate) av_transport: Option<ServiceEndpoint>,
    pub(crate) rendering_control: Option<ServiceEndpoint>,
    /// The local NOTIFY listener backing every GENA subscription this
    /// client makes - one per `RendererClient` regardless of how many
    /// services get subscribed to, started lazily on the first
    /// `subscribe_*` call (a caller only ever using SOAP actions never
    /// pays for it).
    pub(crate) notify: OnceCell<Arc<NotifyListener>>,
}

pub(crate) struct ServiceEndpoint {
    pub(crate) service_type: String,
    pub(crate) control_url: Url,
    pub(crate) event_sub_url: Option<Url>,
}

impl RendererClient {
    /// Fetches `description_url` (a device's `description.xml`) and
    /// resolves whichever of AVTransport/RenderingControl it advertises.
    /// Never fails merely because one of those services is missing -
    /// `av_transport()`/`rendering_control()` report that individually, the
    /// same way a real control point would just not offer transport
    /// controls for a renderer with no AVTransport service.
    pub async fn connect(description_url: &str) -> Result<Self, ClientError> {
        let http = reqwest::Client::new();
        let report = fetch_device_report(&http, description_url).await?;

        let av_transport = resolve_endpoint(find_service(&report.services, AV_TRANSPORT_PREFIX))?;
        let rendering_control =
            resolve_endpoint(find_service(&report.services, RENDERING_CONTROL_PREFIX))?;

        Ok(Self {
            http,
            av_transport,
            rendering_control,
            notify: OnceCell::new(),
        })
    }

    /// The shared NOTIFY listener backing every GENA subscription this
    /// client makes, started on first use.
    pub(crate) async fn notify_listener(&self) -> Result<Arc<NotifyListener>, ClientError> {
        let listener = self.notify.get_or_try_init(NotifyListener::start).await?;
        Ok(Arc::clone(listener))
    }

    pub fn av_transport(&self) -> Result<AVTransportClient<'_>, ClientError> {
        let endpoint = self
            .av_transport
            .as_ref()
            .ok_or(ClientError::ServiceNotFound("AVTransport"))?;
        Ok(AVTransportClient::new(
            &self.http,
            &endpoint.control_url,
            &endpoint.service_type,
        ))
    }

    pub fn rendering_control(&self) -> Result<RenderingControlClient<'_>, ClientError> {
        let endpoint = self
            .rendering_control
            .as_ref()
            .ok_or(ClientError::ServiceNotFound("RenderingControl"))?;
        Ok(RenderingControlClient::new(
            &self.http,
            &endpoint.control_url,
            &endpoint.service_type,
        ))
    }
}

fn find_service<'a>(services: &'a [ServiceReport], prefix: &str) -> Option<&'a ServiceReport> {
    services.iter().find(|s| s.service_type.starts_with(prefix))
}

fn resolve_endpoint(service: Option<&ServiceReport>) -> Result<Option<ServiceEndpoint>, ClientError> {
    let Some(service) = service else {
        return Ok(None);
    };

    let control_url = Url::parse(&service.control_url)
        .map_err(|e| ClientError::InvalidUrl(format!("{}: {e}", service.control_url)))?;
    let event_sub_url = service
        .event_sub_url
        .as_ref()
        .map(|u| Url::parse(u))
        .transpose()
        .map_err(|e| ClientError::InvalidUrl(e.to_string()))?;

    Ok(Some(ServiceEndpoint {
        service_type: service.service_type.clone(),
        control_url,
        event_sub_url,
    }))
}
