//! GENA eventing (UPnP's `SUBSCRIBE`/`UNSUBSCRIBE`/`NOTIFY` mechanism) for
//! AVTransport, RenderingControl and ConnectionManager — lets control
//! points that support it get pushed state the moment it actually changes,
//! instead of having to poll `GetTransportInfo`/`GetVolume`/
//! `GetCurrentConnectionIDs` on a timer.
//!
//! ConnectionManager's `CurrentConnectionIDs` is the one exception that
//! basically never changes in this renderer (always just `"0"`), so it'll
//! rarely if ever produce a NOTIFY after the initial one — but the
//! `SUBSCRIBE` endpoint itself still needs to exist and behave correctly,
//! since `device.xml` advertises an `eventSubURL` for it and a strict
//! control point may depend on getting a real response rather than a 404.

mod handler;
mod last_change;
mod manager;

pub use handler::{
    handle_av_transport_events, handle_connection_manager_events,
    handle_rendering_control_events,
};
pub use last_change::{av_transport_last_change, rendering_control_last_change, AVTransportEventState};
pub use manager::EventManager;
