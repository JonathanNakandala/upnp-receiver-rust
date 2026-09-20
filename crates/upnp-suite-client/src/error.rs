use thiserror::Error;
use upnp_suite_core::discovery::error::DiscoveryServiceError;

/// A UPnP `s:Fault` returned by the device for a rejected/failed action —
/// carries the `UPnPError` code/description a control point is expected to
/// report back, when the fault body includes one (some devices return a
/// bare SOAP fault with no `UPnPError` detail at all).
#[derive(Debug, Clone, Default)]
pub struct SoapFault {
    pub error_code: Option<u32>,
    pub error_description: Option<String>,
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("failed to fetch/parse device description: {0}")]
    Description(#[from] DiscoveryServiceError),
    #[error("device does not advertise a {0} service")]
    ServiceNotFound(&'static str),
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("malformed SOAP response: {0}")]
    MalformedResponse(String),
    #[error("device returned a SOAP fault: {0:?}")]
    Fault(SoapFault),
}
