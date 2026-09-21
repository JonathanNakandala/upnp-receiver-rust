//! `upnpcachehttpsrc` - a bounded in-memory chunk cache sitting in place of
//! `souphttpsrc`'s unbounded `queue2` `download` buffering, for exactly the
//! reason that mode's own doc comment used to justify it: instant seeks
//! into anything already fetched. The difference is this element caps how
//! much that "anything" can be, and keeps it in memory rather than writing
//! the whole asset to disk.
//!
//! See `cache.rs` for the cache itself and `imp.rs` for the element - this
//! module just wires the `glib`/`gst` type machinery and registration.

mod cache;
mod imp;
mod prefetch;

use gstreamer as gst;
use gstreamer_base as gst_base;

use gst::glib;
use gst::glib::subclass::types::ObjectSubclassIsExt;
use gst::prelude::*;

pub use cache::ChunkCache;
pub use imp::Shared;
pub use prefetch::Prefetcher;

glib::wrapper! {
    pub struct CacheHttpSrc(ObjectSubclass<imp::CacheHttpSrc>) @extends gst_base::PushSrc, gst_base::BaseSrc, gst::Element, gst::Object, @implements gst::URIHandler;
}

/// Registers `upnpcachehttpsrc` and demotes the built-in HTTP source(s)'
/// rank below it, so `playbin`/`playbin3`'s normal autoplugging picks this
/// element for `http`/`https` deterministically - not relying on same-rank
/// tie-breaking against `souphttpsrc` (also commonly `PRIMARY`) or
/// `reqwesthttpsrc` (registers itself at `MARGINAL`, already below this,
/// named here anyway so the intent doesn't depend on that staying true).
/// Call once, at process init, alongside `gst::init()`.
pub fn register() -> Result<(), glib::BoolError> {
    gst::Element::register(None, "upnpcachehttpsrc", gst::Rank::PRIMARY, CacheHttpSrc::static_type())?;

    for name in ["souphttpsrc", "reqwesthttpsrc"] {
        if let Some(factory) = gst::ElementFactory::find(name) {
            factory.set_rank(gst::Rank::MARGINAL);
        }
    }

    Ok(())
}

/// Injects the backend-lifetime shared cache/client into `element` if it's
/// an instance of [`CacheHttpSrc`] - a no-op for anything else. Meant to be
/// called from `playbin`'s `source-setup` signal, which fires for every
/// source it creates or reuses regardless of type.
pub fn inject_shared(element: &gst::Element, shared: Shared) {
    if let Some(src) = element.downcast_ref::<CacheHttpSrc>() {
        src.imp().set_shared(shared);
    }
}
