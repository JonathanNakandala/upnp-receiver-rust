//! Exercises the dummy backend's child process end-to-end (spawns the real
//! built binary as `<exe> --backend <port>`, same as `DummyBackendManager`
//! does in production) to check it tracks state and responds consistently
//! even though it never decodes anything real.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use upnp_suite_renderer::backend::dummy::{DummyBackend, DummyBackendManager};
use upnp_suite_renderer::backend::{MediaBackend, MediaBackendError};

async fn new_backend() -> DummyBackend {
    let mut manager =
        DummyBackendManager::new().with_executable_path(env!("CARGO_BIN_EXE_upnp-suite-renderer"));
    manager.start().await.expect("failed to start dummy process");
    DummyBackend::new(Arc::new(Mutex::new(manager)))
}

#[tokio::test]
async fn play_without_media_is_rejected_consistently() {
    let backend = new_backend().await;

    let err = backend.play().await.expect_err("play with nothing loaded must fail");
    assert!(
        matches!(err, MediaBackendError::NoMediaLoaded),
        "expected NoMediaLoaded, got {err:?}"
    );

    // Same error, second time round - state didn't get corrupted by the
    // failed attempt.
    let err = backend.play().await.expect_err("play with nothing loaded must fail");
    assert!(matches!(err, MediaBackendError::NoMediaLoaded));
}

#[tokio::test]
async fn volume_and_mute_round_trip() {
    let backend = new_backend().await;

    assert_eq!(backend.get_volume().await.unwrap(), 100);
    assert!(!backend.get_mute().await.unwrap());

    backend.set_volume(42).await.unwrap();
    assert_eq!(backend.get_volume().await.unwrap(), 42);

    backend.set_mute(true).await.unwrap();
    assert!(backend.get_mute().await.unwrap());

    // Volume is remembered independently of mute, same as GStreamer's
    // playbin properties.
    assert_eq!(backend.get_volume().await.unwrap(), 42);
}

#[tokio::test]
async fn load_then_play_reaches_playing_state() {
    use upnp_suite_renderer::server::soap::types::av_transport::TransportState;

    let backend = new_backend().await;

    backend.load_media("http://example.invalid/track.mp3", None).await.unwrap();
    backend.play().await.unwrap();

    assert_eq!(
        backend.get_playback_state().await.unwrap(),
        TransportState::Playing
    );

    backend.pause().await.unwrap();
    assert_eq!(
        backend.get_playback_state().await.unwrap(),
        TransportState::PausedPlayback
    );

    backend.stop().await.unwrap();
    assert_eq!(
        backend.get_playback_state().await.unwrap(),
        TransportState::Stopped
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
}
