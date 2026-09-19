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
