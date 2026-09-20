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
