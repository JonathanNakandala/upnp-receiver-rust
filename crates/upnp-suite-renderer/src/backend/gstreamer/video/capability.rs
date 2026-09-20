//! Whether this machine can actually display video right now.

use std::sync::OnceLock;
use tracing::{info, warn};

/// A real GPU adapter is available for `wgpu` to render into - not just
/// whether GStreamer has a decoder for some codec installed. Gates whether
/// [`super::super::codecs::supported_mime_types`] advertises video mime
/// types at all: decoding a track nobody can ever see isn't a capability
/// worth offering a control point via `GetProtocolInfo`.
///
/// Checked once and cached - this doesn't change over the life of the
/// process (no display is going to appear or disappear while this binary
/// keeps running), and probing for a real adapter isn't free.
pub(crate) fn can_render() -> bool {
    static CAN_RENDER: OnceLock<bool> = OnceLock::new();
    *CAN_RENDER.get_or_init(|| {
        let adapter = futures::executor::block_on(async {
            let instance = wgpu::Instance::default();
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    force_fallback_adapter: false,
                    // No specific window to be compatible with yet - this is
                    // a capability probe, not the real renderer setup (see
                    // `renderer::Renderer::new`), so it's just asking
                    // whether any adapter exists at all.
                    compatible_surface: None,
                    apply_limit_buckets: false,
                })
                .await
        });

        match adapter {
            Ok(adapter) => {
                info!(
                    adapter = %adapter.get_info().name,
                    backend = ?adapter.get_info().backend,
                    "Video display available; will advertise and show video"
                );
                true
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "No GPU adapter available; video will decode but never be shown, so it won't be advertised either"
                );
                false
            }
        }
    })
}
