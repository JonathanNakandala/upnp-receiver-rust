//! Wall-clock latency regression tests for the operations a UPnP control
//! point is most sensitive to: loading a track, seeking, and switching to a
//! different track. These are *not* correctness tests (see
//! `gstreamer_playback_test.rs` for those) and not classic
//! statistical/criterion-style micro-benchmarks either — each operation
//! here is dominated by real GStreamer/OS/audio-hardware I/O, not CPU work,
//! so there's nothing to usefully average over thousands of iterations.
//! Instead each test measures a small number of real operations once and
//! fails if the result is out of line with a known-good baseline.
//!
//! `#[ignore]`d like the rest of the real-hardware GStreamer suite: run
//! explicitly with
//! `cargo test --test gstreamer_performance_test -- --ignored --nocapture`.
//! `--nocapture` matters here specifically: cargo test discards a passing
//! test's stdout, so the measured numbers are otherwise only visible when a
//! budget is actually exceeded (a failing test's captured output is always
//! shown).
//!
//! Budgets are set with generous headroom over the local baseline noted
//! next to each one (measured on a 2024 MacBook Pro; other machines,
//! especially shared CI runners, can be meaningfully slower) — the goal is
//! to catch a regression that's an order of magnitude off, like the
//! `playbin3` migration that was tried and reverted after this suite's
//! precursor measured ~2.5s for a seek issued right after `play()`
//! (`playbin` does the same in ~7ms), not to chase individual
//! milliseconds. Tighten a budget only with a new measured baseline in
//! hand, and treat a failure here as "go measure what actually got slower"
//! rather than "bump the budget".

mod gstreamer_common;
use gstreamer_common::{fixture_path, generate_test_mp3};
use std::time::{Duration, Instant};
use upnp_suite_renderer::backend::gstreamer::PlaybinVersion;
use upnp_suite_renderer::backend::{GStreamerBackend, MediaBackend};

/// Baseline: ~7ms average over 5 seeks once prerolled. Budget gives >20x
/// headroom.
const SEEK_LATENCY_BUDGET: Duration = Duration::from_millis(150);

/// Baseline: ~15ms to learn the duration of a bare MP3 (no Xing/VBRI
/// header) once bytes start flowing, on a backend that's already played
/// something (see the warm-up step in
/// `load_track_latency_stays_within_budget` — a process's *first-ever*
/// GStreamer playback pays a one-time registry-scan/audio-device-open cost
/// an order of magnitude higher than this, but that only happens once at
/// app startup). Budget gives ~20x headroom.
const LOAD_READY_BUDGET: Duration = Duration::from_millis(300);

/// A track switch (skipping to a different, non-queued track) is a
/// `load_media` + `play` round trip from this backend's perspective — see
/// the doc comment on `track_switch_latency_stays_within_budget`. Baseline:
/// ~90ms (slower than `LOAD_READY_BUDGET`'s baseline since it's a distinct
/// file the OS hasn't just read). Budget gives >5x headroom.
const TRACK_SWITCH_BUDGET: Duration = Duration::from_millis(500);

const POLL_INTERVAL: Duration = Duration::from_millis(5);
const POLL_TIMEOUT: Duration = Duration::from_secs(5);

fn report(name: &str, elapsed: Duration, budget: Duration) {
    println!("{name}: {elapsed:?} (budget {budget:?})");
    assert!(
        elapsed <= budget,
        "{name} took {elapsed:?}, which exceeds the {budget:?} regression budget"
    );
}

#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test --test gstreamer_performance_test -- --ignored --nocapture`"]
async fn load_track_latency_stays_within_budget() {
    let path = fixture_path("perf_load");
    generate_test_mp3(&path, 50);
    let uri = format!("file://{}", path.display());

    let backend = GStreamerBackend::new().unwrap();

    // Warm-up load, not measured: a *process's* first-ever GStreamer
    // playback pays for one-time costs unrelated to this backend's own
    // code — plugin registry scan, opening the OS audio device, spinning
    // up GStreamer's internal thread pool. In the real app that only
    // happens once, at startup (`GStreamerBackend::new()` is called once
    // in `main.rs`); every subsequent "load a track" during a session is
    // warm, which is the scenario this budget is meant to guard.
    backend.load_media(&uri, None).await.unwrap();
    backend.play().await.unwrap();
    while backend.get_duration().await.unwrap().is_none() {
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    backend.stop().await.unwrap();

    let start = Instant::now();
    backend.load_media(&uri, None).await.unwrap();
    backend.play().await.unwrap();
    loop {
        if backend.get_duration().await.unwrap().is_some() {
            break;
        }
        assert!(
            start.elapsed() < POLL_TIMEOUT,
            "duration never became known"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    report(
        "load_track (time to duration known)",
        start.elapsed(),
        LOAD_READY_BUDGET,
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test --test gstreamer_performance_test -- --ignored --nocapture`"]
async fn seek_latency_stays_within_budget() {
    // ~11.5s (500 buffers), so there's room to seek to several distinct
    // positions.
    let path = fixture_path("perf_seek");
    generate_test_mp3(&path, 500);
    let uri = format!("file://{}", path.display());

    let backend = GStreamerBackend::new().unwrap();
    backend.load_media(&uri, None).await.unwrap();
    backend.play().await.unwrap();

    // Let preroll settle first — this test is about steady-state seek cost,
    // not preroll (that's `load_track_latency_stays_within_budget`'s job,
    // and mixing the two in would make a seek regression indistinguishable
    // from a load regression).
    let ready_start = Instant::now();
    while backend.get_duration().await.unwrap().is_none() {
        assert!(
            ready_start.elapsed() < POLL_TIMEOUT,
            "duration never became known"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    let iterations: u32 = 5;
    let mut total = Duration::ZERO;
    for i in 0..iterations {
        let target = Duration::from_secs(1 + i as u64);
        let start = Instant::now();
        backend.seek(target).await.unwrap();
        loop {
            let pos = backend.get_position().await.unwrap();
            if pos >= target.saturating_sub(Duration::from_millis(200)) {
                break;
            }
            assert!(start.elapsed() < POLL_TIMEOUT, "seek never landed");
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        total += start.elapsed();
    }
    report(
        "seek (average of 5, once prerolled)",
        total / iterations,
        SEEK_LATENCY_BUDGET,
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test --test gstreamer_performance_test -- --ignored --nocapture`"]
async fn track_switch_latency_stays_within_budget() {
    // "Previous"/"Next" have no dedicated backend mechanism beyond the
    // single `SetNextAVTransportURI` gapless lookahead slot (see
    // `MediaBackend::set_next_media`) — a control point skipping to an
    // arbitrary track (i.e. not the one queued for a gapless transition)
    // does it the same way loading any new track does:
    // `SetAVTransportURI` + `Play`. So that's what this measures. The
    // gapless path isn't benchmarked the same way because its trigger
    // timing (GStreamer's `about-to-finish`, fired a few seconds before
    // EOS based on internal buffering heuristics) isn't something this
    // backend's own code controls — see `gapless_transition_promotes_queued_next_track`
    // in `gstreamer_playback_test.rs` for its correctness coverage instead.
    let path_a = fixture_path("perf_switch_a");
    let path_b = fixture_path("perf_switch_b");
    generate_test_mp3(&path_a, 50);
    generate_test_mp3(&path_b, 50);
    let uri_a = format!("file://{}", path_a.display());
    let uri_b = format!("file://{}", path_b.display());

    let backend = GStreamerBackend::new().unwrap();
    backend.load_media(&uri_a, None).await.unwrap();
    backend.play().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let start = Instant::now();
    backend.load_media(&uri_b, None).await.unwrap();
    backend.play().await.unwrap();
    loop {
        // Position resets on a fresh `load_media`, so seeing it advance
        // again is proof the new track is actually flowing, not just that
        // the switch was requested.
        if backend.get_position().await.unwrap() > Duration::ZERO {
            break;
        }
        assert!(start.elapsed() < POLL_TIMEOUT, "switch never completed");
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    report("track switch", start.elapsed(), TRACK_SWITCH_BUDGET);

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}

/// Direct comparison, not a regression gate: prints each pipeline's average
/// seek latency over a long track instead of asserting a budget, since the
/// point here is to see the numbers side by side, not fail CI on either
/// one. Run with `cargo test --test gstreamer_performance_test -- --ignored --nocapture compares_playbin`.
///
/// Runs both pipelines sequentially in one test (not two parallel ones):
/// several real GStreamer audio pipelines playing concurrently on this
/// machine contend for the same audio device and are unreliable in
/// parallel - run this test file with `--test-threads=1` if running it
/// alongside the rest of this suite.
#[tokio::test]
#[ignore = "spins up real GStreamer audio playback on this machine; run with `cargo test --test gstreamer_performance_test -- --ignored --nocapture`"]
async fn compares_playbin_and_playbin3_seek_latency_across_a_long_track() {
    // ~180s (3 minutes) at ~23ms/buffer - long enough that seeking to
    // widely spaced positions actually exercises buffering behavior, which
    // a few-second clip wouldn't (see `PlaybinVersion::V3`'s docs -
    // `playbin3` buffers demuxed data by default, `playbin` doesn't).
    let path = fixture_path("perf_compare_playbin");
    generate_test_mp3(&path, 7826);
    let uri = format!("file://{}", path.display());

    let targets = [
        Duration::from_secs(20),
        Duration::from_secs(60),
        Duration::from_secs(100),
        Duration::from_secs(140),
        Duration::from_secs(170),
    ];

    let v1 = average_seek_latency(PlaybinVersion::V1, &uri, &targets).await;
    let v3 = average_seek_latency(PlaybinVersion::V3, &uri, &targets).await;

    println!("playbin  (v1): {v1:?} average over {} seeks", targets.len());
    println!("playbin3 (v3): {v3:?} average over {} seeks", targets.len());

    let _ = std::fs::remove_file(&path);
}

/// Loads `uri`, waits for duration to be known, then seeks to each of
/// `targets` in turn and returns the average time for the position to
/// reflect the seek - same landing check `seek_latency_stays_within_budget`
/// uses.
async fn average_seek_latency(
    version: PlaybinVersion,
    uri: &str,
    targets: &[Duration],
) -> Duration {
    let backend = GStreamerBackend::new_with_playbin_version(version).unwrap();
    backend.load_media(uri, None).await.unwrap();
    backend.play().await.unwrap();

    let ready_start = Instant::now();
    while backend.get_duration().await.unwrap().is_none() {
        assert!(
            ready_start.elapsed() < POLL_TIMEOUT,
            "duration never became known ({version:?})"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    let mut total = Duration::ZERO;
    for &target in targets {
        let start = Instant::now();
        backend.seek(target).await.unwrap();
        loop {
            let pos = backend.get_position().await.unwrap();
            if pos >= target.saturating_sub(Duration::from_millis(200)) {
                break;
            }
            assert!(
                start.elapsed() < POLL_TIMEOUT,
                "seek never landed ({version:?})"
            );
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        total += start.elapsed();
    }

    backend.stop().await.unwrap();
    total / targets.len() as u32
}
