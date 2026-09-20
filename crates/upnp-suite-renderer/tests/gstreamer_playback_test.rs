//! Correctness tests for the real GStreamer backend that need actual audio
//! playback on this machine — as opposed to the fast, hardware-independent
//! unit tests in `backend/gstreamer/backend.rs`'s own `#[cfg(test)] mod
//! tests` (state-only checks like volume/mute round-tripping, which don't
//! need a real pipeline to actually play anything).
//!
//! These only exercise `GStreamerBackend` through its public `MediaBackend`
//! API, so they belong here rather than as unit tests — same convention as
//! this crate's other `tests/*.rs` integration suites.
//!
//! For wall-clock latency budgets (seek/load/track-switch performance
//! regressions), see `gstreamer_performance_test.rs`.

mod gstreamer_common;
use gstreamer_common::{fixture_path, generate_test_mp3};
use std::time::Duration;
use upnp_suite_renderer::backend::{GStreamerBackend, MediaBackend, TrackInfo};
use upnp_suite_renderer::server::soap::types::av_transport::{PlayMode, TransportState};

#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test -- --ignored`"]
async fn play_at_default_speed_does_not_race_initial_preroll() {
    // Regression test: AVTransport's Play handler always reissues
    // set_play_speed(1.0) right after play(), even when the control point
    // never asked for anything but normal speed (BubbleUPnP, among others,
    // always sends Speed=1). Before the no-op skip in set_play_speed, this
    // raced the pipeline's *first* preroll — triggered here by calling both
    // back-to-back with no delay in between — and playbin's rate-change
    // seek intermittently failed with "Failed to seek" on a fresh track.
    let path = fixture_path("speed_race");
    generate_test_mp3(&path, 50);
    let uri = format!("file://{}", path.display());

    let backend = GStreamerBackend::new().unwrap();

    for _ in 0..20 {
        backend
            .load_media(&uri, Some("test track".to_string()))
            .await
            .unwrap();
        backend.play().await.unwrap();
        backend.set_play_speed(1.0).await.unwrap();
        backend.stop().await.unwrap();
    }

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test -- --ignored`"]
async fn plays_a_real_mp3_end_to_end() {
    let path = fixture_path("play_end_to_end");
    generate_test_mp3(&path, 50);
    let uri = format!("file://{}", path.display());

    let backend = GStreamerBackend::new().unwrap();
    backend
        .load_media(&uri, Some("test track".to_string()))
        .await
        .unwrap();

    backend.play().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        backend.get_playback_state().await.unwrap(),
        TransportState::Playing
    );

    // Our test fixture is a bare MP3 (no Xing/VBRI header), so duration
    // isn't resolved until bytes are actually flowing — check it here,
    // after Play, rather than right after load.
    let duration = backend.get_duration().await.unwrap();
    assert!(
        duration.is_some(),
        "playbin should know the duration once data is flowing"
    );

    let position = backend.get_position().await.unwrap();
    assert!(
        position > Duration::ZERO,
        "position should have advanced while playing, got {:?}",
        position
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

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test -- --ignored`"]
async fn reaches_stopped_on_natural_end_of_stream() {
    // The fixture is short (~1s of audio), so playback reaches EOS quickly
    // without needing a dedicated shorter one.
    let path = fixture_path("eos");
    generate_test_mp3(&path, 50);
    let uri = format!("file://{}", path.display());

    let backend = GStreamerBackend::new().unwrap();
    let mut state_changes = backend.state_changes();

    backend
        .load_media(&uri, Some("test track".to_string()))
        .await
        .unwrap();
    backend.play().await.unwrap();

    // The bus watcher polls the pipeline bus on its own task, so wait on
    // its notification channel rather than the transport state directly —
    // avoids a fixed sleep racing the actual EOS timing.
    tokio::time::timeout(Duration::from_secs(5), state_changes.changed())
        .await
        .expect("bus watcher should report a state change before the timeout")
        .unwrap();

    assert_eq!(
        backend.get_playback_state().await.unwrap(),
        TransportState::Stopped,
        "reaching end-of-stream on its own should be reported the same as an explicit stop()"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test -- --ignored`"]
async fn gapless_transition_promotes_queued_next_track() {
    let path_a = fixture_path("gapless_a");
    let path_b = fixture_path("gapless_b");
    generate_test_mp3(&path_a, 50);
    generate_test_mp3(&path_b, 50);
    let uri_a = format!("file://{}", path_a.display());
    let uri_b = format!("file://{}", path_b.display());

    let backend = GStreamerBackend::new().unwrap();
    let mut track_changes = backend.track_changes();

    backend
        .load_media(&uri_a, Some("track a".to_string()))
        .await
        .unwrap();
    backend
        .set_next_media(&uri_b, Some("track b".to_string()))
        .await
        .unwrap();
    assert_eq!(
        backend.get_next_media().await.unwrap(),
        Some(TrackInfo {
            uri: uri_b.clone(),
            metadata: Some("track b".to_string()),
        })
    );

    backend.play().await.unwrap();

    // `about-to-finish` fires on its own once GStreamer decides it's
    // running low on queued data for the current track — wait on the
    // notification rather than a fixed sleep, same reasoning as the EOS
    // test above.
    tokio::time::timeout(Duration::from_secs(5), track_changes.changed())
        .await
        .expect("about-to-finish should promote the queued track before the timeout")
        .unwrap();

    assert_eq!(
        backend.get_current_uri().await.unwrap(),
        Some(uri_b),
        "current URI should have switched to the queued next track"
    );
    assert_eq!(
        backend.get_next_media().await.unwrap(),
        None,
        "the queued slot should be consumed once promoted"
    );
    assert_eq!(
        backend.get_playback_state().await.unwrap(),
        TransportState::Playing,
        "a gapless transition shouldn't visibly stop playback in between"
    );

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}

#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test -- --ignored`"]
async fn repeat_one_loops_current_track_with_no_explicit_next() {
    let path = fixture_path("repeat_one");
    generate_test_mp3(&path, 50);
    let uri = format!("file://{}", path.display());

    let backend = GStreamerBackend::new().unwrap();
    let mut track_changes = backend.track_changes();

    backend
        .load_media(&uri, Some("looping track".to_string()))
        .await
        .unwrap();
    backend.set_play_mode(PlayMode::RepeatOne).await.unwrap();
    assert_eq!(backend.get_play_mode().await.unwrap(), PlayMode::RepeatOne);

    backend.play().await.unwrap();

    // No explicit SetNextAVTransportURI was ever queued, so the only thing
    // that can fire `track_changes` here is `about-to-finish` consulting
    // `CurrentPlayMode` and re-queueing the same track.
    tokio::time::timeout(Duration::from_secs(5), track_changes.changed())
        .await
        .expect("about-to-finish should loop the track before the timeout")
        .unwrap();

    assert_eq!(
        backend.get_current_uri().await.unwrap(),
        Some(uri),
        "current URI should still be the same track, looped rather than stopped"
    );
    assert_eq!(
        backend.get_playback_state().await.unwrap(),
        TransportState::Playing,
        "looping gaplessly shouldn't visibly stop playback in between"
    );

    let _ = std::fs::remove_file(&path);
}
