use test_log::test;
mod common;
use common::{SoapClient, TestServer};

use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

fn init_logging() {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("upnp_receiver=debug".parse().unwrap())
                .add_directive("test=debug".parse().unwrap()),
        )
        .with_span_events(FmtSpan::FULL)
        .with_test_writer() // This ensures logs go to the test output
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .pretty()
        .try_init();

    // It's okay if this fails - might already be initialized
    let _ = subscriber;
}

// Each test gets its own server bound to an OS-assigned ephemeral port (port 0),
// so tests can run concurrently without sharing playback state or racing on shutdown.
async fn setup() -> (TestServer, SoapClient) {
    init_logging();
    let server = TestServer::new().await;
    assert!(
        server.wait_for_ready().await,
        "Server failed to become ready"
    );
    let client = SoapClient::new(server.address);
    (server, client)
}

// Helper function to check response and print detailed error if needed
async fn assert_response(resp: reqwest::Response, expected_status: u16) {
    let status = resp.status().as_u16();
    if status != expected_status {
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "Failed to get response body".to_string());
        panic!(
            "Expected status {}, got {}. Response body: {}",
            expected_status, status, body
        );
    }
}

#[test(tokio::test)]
async fn test_set_transport_uri() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;
}

#[test(tokio::test)]
async fn test_basic_playback_flow() {
    let (_server, client) = setup().await;

    // Set media
    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    // Play
    let resp = client.play().await;
    assert_response(resp, 200).await;

    // Get transport info and verify state
    let resp = client.get_transport_info().await;
    assert_response(resp, 200).await;
    // TODO: Add verification of actual transport state from response body

    // Pause
    let resp = client.pause().await;
    assert_response(resp, 200).await;

    // Play again
    let resp = client.play().await;
    assert_response(resp, 200).await;

    // Stop
    let resp = client.stop().await;
    assert_response(resp, 200).await;
}

#[test(tokio::test)]
async fn test_invalid_state_transitions() {
    let (_server, client) = setup().await;

    // Try to play without setting media first
    let resp = client.play().await;
    assert_eq!(resp.status(), 500); // Or whatever error code you return for invalid state transitions
}

// Additional test cases

#[test(tokio::test)]
async fn test_pause_from_stopped_state() {
    let (_server, client) = setup().await;

    let resp = client.pause().await;
    assert_response(resp, 500).await; // Should fail when trying to pause while stopped
}

#[test(tokio::test)]
async fn test_double_play() {
    let (_server, client) = setup().await;

    // Set media and start playing
    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.play().await;
    assert_response(resp, 200).await;

    // Try to play again while already playing
    let resp = client.play().await;
    assert_response(resp, 200).await; // Should return success as it's idempotent
}

#[test(tokio::test)]
async fn test_seek_without_media() {
    let (_server, client) = setup().await;

    let resp = client.seek("00:01:00").await;
    assert_response(resp, 500).await; // No media loaded to seek within
}

#[test(tokio::test)]
async fn test_seek_rejects_invalid_target() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.seek("not-a-time").await;
    assert_response(resp, 400).await;
}

#[test(tokio::test)]
async fn test_seek_updates_position() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.play().await;
    assert_response(resp, 200).await;

    let resp = client.seek("00:01:30").await;
    assert_response(resp, 200).await;

    let resp = client.get_position_info().await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 200, "Response body: {}", body);
    assert!(
        body.contains("<RelTime>00:01:30</RelTime>"),
        "Seek did not update reported position: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_transport_uri_while_playing() {
    let (_server, client) = setup().await;

    // Set initial media and play
    let resp = client
        .set_av_transport_uri("http://example.com/test1.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.play().await;
    assert_response(resp, 200).await;

    // Try to set new URI while playing
    let resp = client
        .set_av_transport_uri("http://example.com/test2.mp3", None)
        .await;
    assert_response(resp, 200).await; // Should succeed and stop current playback
}

#[test(tokio::test)]
async fn test_get_transport_settings_defaults_to_normal() {
    let (_server, client) = setup().await;

    let resp = client.get_transport_settings().await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 200, "Response body: {}", body);
    assert!(body.contains("<PlayMode>NORMAL</PlayMode>"));
    assert!(body.contains("<RecQualityMode>NOT_IMPLEMENTED</RecQualityMode>"));
}

#[test(tokio::test)]
async fn test_set_play_mode_updates_transport_settings() {
    let (_server, client) = setup().await;

    let resp = client.set_play_mode("REPEAT_ONE").await;
    assert_response(resp, 200).await;

    let resp = client.get_transport_settings().await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 200, "Response body: {}", body);
    assert!(body.contains("<PlayMode>REPEAT_ONE</PlayMode>"));
}

#[test(tokio::test)]
async fn test_set_play_mode_rejects_unsupported_value() {
    let (_server, client) = setup().await;

    let resp = client.set_play_mode("INTRO").await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 400, "Response body: {}", body);
    assert!(body.contains("<errorCode>712</errorCode>"));
}

#[test(tokio::test)]
async fn test_get_current_transport_actions_when_stopped() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.get_current_transport_actions().await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 200, "Response body: {}", body);
    assert!(body.contains("<Actions>Play</Actions>"));
}

#[test(tokio::test)]
async fn test_get_current_transport_actions_when_playing() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.play().await;
    assert_response(resp, 200).await;

    let resp = client.get_current_transport_actions().await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 200, "Response body: {}", body);
    assert!(body.contains("<Actions>Pause,Stop,Seek</Actions>"));
}

#[test(tokio::test)]
async fn test_get_current_transport_actions_with_no_media() {
    let (_server, client) = setup().await;

    let resp = client.get_current_transport_actions().await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 200, "Response body: {}", body);
    assert!(
        body.contains("<Actions/>") || body.contains("<Actions></Actions>"),
        "Response body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_device_capabilities() {
    let (_server, client) = setup().await;

    let resp = client.get_device_capabilities().await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 200, "Response body: {}", body);
    assert!(body.contains("<PlayMedia>NETWORK,NONE</PlayMedia>"));
    assert!(body.contains("<RecMedia>NOT_IMPLEMENTED</RecMedia>"));
    assert!(body.contains("<RecQualityModes>NOT_IMPLEMENTED</RecQualityModes>"));
}

#[test(tokio::test)]
async fn test_next_without_queued_track_fails() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    // Nothing queued via SetNextAVTransportURI, so there's nowhere for Next
    // to go.
    let resp = client.next().await;
    assert_response(resp, 500).await;
}

#[test(tokio::test)]
async fn test_next_promotes_queued_track() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/track1.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client
        .set_next_av_transport_uri("http://example.com/track2.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.next().await;
    assert_response(resp, 200).await;

    let resp = client.get_media_info().await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 200, "Response body: {}", body);
    assert!(
        body.contains("<CurrentURI>http://example.com/track2.mp3</CurrentURI>"),
        "Response body: {}",
        body
    );
    // The lookahead slot is single-use — nothing left queued after promotion.
    assert!(
        body.contains("<NextURI></NextURI>") || body.contains("<NextURI/>"),
        "Response body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_previous_is_never_supported() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    // No history/queue behind the current track exists in this renderer, so
    // Previous always reports 701 (Transition Not Available) rather than
    // faking a "restart current track" behavior.
    let resp = client.previous().await;
    assert_response(resp, 500).await;
}

#[test(tokio::test)]
async fn test_current_transport_actions_lists_next_only_when_queued() {
    let (_server, client) = setup().await;

    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.get_current_transport_actions().await;
    let body = resp.text().await.unwrap();
    assert!(body.contains("<Actions>Play</Actions>"));

    let resp = client
        .set_next_av_transport_uri("http://example.com/track2.mp3", None)
        .await;
    assert_response(resp, 200).await;

    let resp = client.get_current_transport_actions().await;
    let body = resp.text().await.unwrap();
    assert!(body.contains("<Actions>Play,Next</Actions>"), "Response body: {}", body);
}
