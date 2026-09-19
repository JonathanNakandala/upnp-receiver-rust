use test_log::test;
mod common;
use common::{SoapClient, TestServer};

async fn setup() -> (TestServer, SoapClient) {
    let server = TestServer::new().await;
    assert!(
        server.wait_for_ready().await,
        "Server failed to become ready"
    );
    let client = SoapClient::new(server.address);
    (server, client)
}

async fn assert_response(resp: reqwest::Response, expected_status: u16) -> String {
    let status = resp.status().as_u16();
    let body = resp
        .text()
        .await
        .unwrap_or_else(|_| "Failed to get response body".to_string());
    assert_eq!(
        status, expected_status,
        "Expected status {}, got {}. Response body: {}",
        expected_status, status, body
    );
    body
}

#[test(tokio::test)]
async fn test_get_volume_default() {
    let (_server, client) = setup().await;

    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>50</CurrentVolume>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_volume_changes_get_volume() {
    let (_server, client) = setup().await;

    let resp = client.set_volume(75).await;
    assert_response(resp, 200).await;

    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>75</CurrentVolume>"),
        "Volume change was not reflected: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_volume_rejects_out_of_range() {
    let (_server, client) = setup().await;

    let resp = client.set_volume(150).await;
    assert_response(resp, 500).await;

    // State should be unchanged after a rejected update
    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>50</CurrentVolume>"),
        "Volume should not have changed after invalid SetVolume: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_volume_db_range() {
    let (_server, client) = setup().await;

    let resp = client.get_volume_db_range().await;
    let body = assert_response(resp, 200).await;
    assert!(body.contains("<MinValue>-60</MinValue>"), "body: {}", body);
    assert!(body.contains("<MaxValue>0</MaxValue>"), "body: {}", body);
}

#[test(tokio::test)]
async fn test_get_volume_db_matches_default_volume() {
    let (_server, client) = setup().await;

    // Default volume is 50%, which is -6dB.
    let resp = client.get_volume_db().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>-6</CurrentVolume>"),
        "body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_volume_db_updates_volume_and_volume_db() {
    let (_server, client) = setup().await;

    let resp = client.set_volume_db(0).await;
    assert_response(resp, 200).await;

    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>100</CurrentVolume>"),
        "body: {}",
        body
    );

    let resp = client.get_volume_db().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>0</CurrentVolume>"),
        "body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_volume_db_rejects_out_of_range() {
    let (_server, client) = setup().await;

    let resp = client.set_volume_db(1).await;
    assert_response(resp, 500).await;

    let resp = client.set_volume_db(-61).await;
    assert_response(resp, 500).await;
}

#[test(tokio::test)]
async fn test_get_mute_default() {
    let (_server, client) = setup().await;

    let resp = client.get_mute().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentMute>false</CurrentMute>")
            || body.contains("<CurrentMute>0</CurrentMute>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_mute_changes_get_mute() {
    let (_server, client) = setup().await;

    let resp = client.set_mute(true).await;
    assert_response(resp, 200).await;

    let resp = client.get_mute().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentMute>true</CurrentMute>")
            || body.contains("<CurrentMute>1</CurrentMute>"),
        "Mute change was not reflected: {}",
        body
    );

    // Toggle back
    let resp = client.set_mute(false).await;
    assert_response(resp, 200).await;

    let resp = client.get_mute().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentMute>false</CurrentMute>")
            || body.contains("<CurrentMute>0</CurrentMute>"),
        "Mute change was not reflected: {}",
        body
    );
}
