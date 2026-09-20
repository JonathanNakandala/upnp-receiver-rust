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
async fn test_get_protocol_info() {
    let (_server, client) = setup().await;

    let resp = client.get_protocol_info().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<Sink>http-get:*:audio/mpeg:DLNA.ORG_PN=MP3</Sink>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_current_connection_ids() {
    let (_server, client) = setup().await;

    let resp = client.get_current_connection_ids().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<ConnectionIDs>0</ConnectionIDs>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_current_connection_info_known_id() {
    let (_server, client) = setup().await;

    let resp = client.get_current_connection_info(0).await;
    let body = assert_response(resp, 200).await;
    assert!(body.contains("<Direction>Input</Direction>"), "{}", body);
    assert!(body.contains("<Status>OK</Status>"), "{}", body);
}

#[test(tokio::test)]
async fn test_get_current_connection_info_unknown_id() {
    let (_server, client) = setup().await;

    let resp = client.get_current_connection_info(99).await;
    assert_response(resp, 400).await;
}

#[test(tokio::test)]
async fn test_get_protocol_info_backward_compatible_v1_header() {
    let (_server, client) = setup().await;

    // The service now advertises ConnectionManager:3, but a control point
    // that only knows about :1 must still be served.
    let resp = client.get_protocol_info_v1().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<Sink>http-get:*:audio/mpeg:DLNA.ORG_PN=MP3</Sink>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_feature_list() {
    let (_server, client) = setup().await;

    let resp = client.get_feature_list().await;
    let body = assert_response(resp, 200).await;
    assert!(body.contains("<FeatureList>"), "Unexpected body: {}", body);
    assert!(body.contains("&lt;Features"), "Unexpected body: {}", body);
}

#[test(tokio::test)]
async fn test_prepare_for_connection_and_connection_complete() {
    let (_server, client) = setup().await;

    let resp = client
        .prepare_for_connection("http-get:*:audio/mpeg:*", "Input")
        .await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<AVTransportID>0</AVTransportID>"),
        "{}",
        body
    );
    assert!(body.contains("<RcsID>0</RcsID>"), "{}", body);

    // Pull the allocated ConnectionID back out of the response body.
    let connection_id: i32 = body
        .split("<ConnectionID>")
        .nth(1)
        .and_then(|s| s.split("</ConnectionID>").next())
        .and_then(|s| s.parse().ok())
        .expect("response should contain a ConnectionID");
    assert_ne!(connection_id, 0);

    // The new connection now shows up alongside the implicit default one.
    let resp = client.get_current_connection_ids().await;
    let body = assert_response(resp, 200).await;
    assert!(body.contains(&format!("0,{}", connection_id)), "{}", body);

    let resp = client.connection_complete(connection_id).await;
    assert_response(resp, 200).await;

    let resp = client.get_current_connection_ids().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<ConnectionIDs>0</ConnectionIDs>"),
        "{}",
        body
    );
}

#[test(tokio::test)]
async fn test_prepare_for_connection_rejects_output_direction() {
    let (_server, client) = setup().await;

    let resp = client
        .prepare_for_connection("http-get:*:audio/mpeg:*", "Output")
        .await;
    assert_response(resp, 500).await;
}

#[test(tokio::test)]
async fn test_connection_complete_rejects_unknown_id() {
    let (_server, client) = setup().await;

    let resp = client.connection_complete(99).await;
    assert_response(resp, 400).await;
}
