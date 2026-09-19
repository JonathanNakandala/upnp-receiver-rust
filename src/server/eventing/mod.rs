//! GENA eventing (UPnP's `SUBSCRIBE`/`UNSUBSCRIBE`/`NOTIFY` mechanism) for
//! AVTransport and RenderingControl — lets control points that support it
//! get pushed a `LastChange` NOTIFY the moment state actually changes,
//! instead of having to poll `GetTransportInfo`/`GetVolume` on a timer.
//!
//! ConnectionManager is deliberately left out: its state barely ever
//! changes, so there's no responsiveness win worth the extra subscriber
//! bookkeeping.

mod handler;
mod last_change;
mod manager;

pub use handler::{handle_av_transport_events, handle_rendering_control_events};
pub use last_change::{av_transport_last_change, rendering_control_last_change};
pub use manager::EventManager;
