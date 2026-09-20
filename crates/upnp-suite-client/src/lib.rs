//! A general-purpose UPnP MediaRenderer control point: SOAP action
//! invocation plus (in a later phase) GENA subscribe/NOTIFY, usable both
//! standalone (`upnp-suite-cli`) and by `upnp-suite-renderer`'s own video
//! window menu talking to its own HTTP endpoint over loopback - the same
//! client code either way, so a menu click is indistinguishable, to the
//! renderer, from a real control point's.

mod av_transport;
mod device;
mod error;
mod eventing;
mod last_change;
mod rendering_control;
mod soap;
mod xml;

pub use av_transport::{AVTransportClient, TransportInfo};
pub use device::RendererClient;
pub use error::{ClientError, SoapFault};
pub use eventing::EventStream;
pub use last_change::{AVTransportSnapshot, RenderingControlSnapshot};
pub use rendering_control::RenderingControlClient;
pub use soap::invoke_action;
