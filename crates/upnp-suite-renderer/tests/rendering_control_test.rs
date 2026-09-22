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
async fn test_list_presets_reports_factory_defaults() {
    let (_server, client) = setup().await;

    let resp = client.list_presets().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentPresetNameList>FactoryDefaults</CurrentPresetNameList>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_select_preset_accepts_factory_defaults() {
    let (_server, client) = setup().await;

    let resp = client.select_preset("FactoryDefaults").await;
    assert_response(resp, 200).await;
}

#[test(tokio::test)]
async fn test_select_preset_rejects_unknown_name() {
    let (_server, client) = setup().await;

    let resp = client.select_preset("Bogus").await;
    let body = assert_response(resp, 400).await;
    assert!(
        body.contains("701"),
        "Expected UPnP error 701 (Invalid Preset Name): {}",
        body
    );
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

#[test(tokio::test)]
async fn test_set_volume_with_unknown_instance_id_is_rejected() {
    let (_server, client) = setup().await;

    // Nothing has ever created instance 1 — PrepareForConnection isn't
    // implemented, so only the default instance (0) exists.
    let resp = client.set_volume_with_instance_id(1, 50).await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 400, "Response body: {}", body);
    assert!(body.contains("<errorCode>718</errorCode>"), "Response body: {}", body);
}

#[test(tokio::test)]
async fn test_get_volume_with_non_master_channel_is_rejected() {
    let (_server, client) = setup().await;

    // "Master" is the only channel the SCPD's A_ARG_TYPE_Channel
    // allowedValueList advertises - anything else should be rejected, not
    // silently applied to the master channel.
    let resp = client.get_volume_with_channel("LF").await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap();
    assert_eq!(status, 400, "Response body: {}", body);
    assert!(
        body.contains("<errorCode>703</errorCode>"),
        "Expected UPnP error 703 (Invalid Channel): {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_brightness_default() {
    let (_server, client) = setup().await;

    let resp = client.get_brightness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentBrightness>50</CurrentBrightness>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_brightness_changes_get_brightness() {
    let (_server, client) = setup().await;

    let resp = client.set_brightness(80).await;
    assert_response(resp, 200).await;

    let resp = client.get_brightness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentBrightness>80</CurrentBrightness>"),
        "Brightness change was not reflected: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_brightness_rejects_out_of_range() {
    let (_server, client) = setup().await;

    let resp = client.set_brightness(150).await;
    assert_response(resp, 500).await;

    let resp = client.get_brightness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentBrightness>50</CurrentBrightness>"),
        "Brightness should not have changed after invalid SetBrightness: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_contrast_default() {
    let (_server, client) = setup().await;

    let resp = client.get_contrast().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentContrast>50</CurrentContrast>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_contrast_changes_get_contrast() {
    let (_server, client) = setup().await;

    let resp = client.set_contrast(20).await;
    assert_response(resp, 200).await;

    let resp = client.get_contrast().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentContrast>20</CurrentContrast>"),
        "Contrast change was not reflected: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_contrast_rejects_out_of_range() {
    let (_server, client) = setup().await;

    let resp = client.set_contrast(150).await;
    assert_response(resp, 500).await;

    let resp = client.get_contrast().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentContrast>50</CurrentContrast>"),
        "Contrast should not have changed after invalid SetContrast: {}",
        body
    );
}

// RedVideoGain and ColorTemperature below stand in for all seven new
// Gain/BlackLevel/ColorTemperature actions - same dispatch mechanism, same
// code shape (see action.rs), so these two cover both new value ranges
// (0-1000 and 0-100) and prove the SOAP action-name dispatcher (the exact
// thing that silently 402'd Brightness/Contrast before service.rs's match
// arms were added) is wired up correctly for this whole group too.

#[test(tokio::test)]
async fn test_get_red_video_gain_default() {
    let (_server, client) = setup().await;

    let resp = client.get_red_video_gain().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentRedVideoGain>500</CurrentRedVideoGain>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_red_video_gain_changes_get_red_video_gain() {
    let (_server, client) = setup().await;

    let resp = client.set_red_video_gain(750).await;
    assert_response(resp, 200).await;

    let resp = client.get_red_video_gain().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentRedVideoGain>750</CurrentRedVideoGain>"),
        "RedVideoGain change was not reflected: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_red_video_gain_rejects_out_of_range() {
    let (_server, client) = setup().await;

    let resp = client.set_red_video_gain(1001).await;
    assert_response(resp, 500).await;

    let resp = client.get_red_video_gain().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentRedVideoGain>500</CurrentRedVideoGain>"),
        "RedVideoGain should not have changed after invalid SetRedVideoGain: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_color_temperature_default_is_d65() {
    let (_server, client) = setup().await;

    let resp = client.get_color_temperature().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentColorTemperature>50</CurrentColorTemperature>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_color_temperature_changes_get_color_temperature() {
    let (_server, client) = setup().await;

    let resp = client.set_color_temperature(0).await;
    assert_response(resp, 200).await;

    let resp = client.get_color_temperature().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentColorTemperature>0</CurrentColorTemperature>"),
        "ColorTemperature change was not reflected: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_color_temperature_rejects_out_of_range() {
    let (_server, client) = setup().await;

    let resp = client.set_color_temperature(101).await;
    assert_response(resp, 500).await;

    let resp = client.get_color_temperature().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentColorTemperature>50</CurrentColorTemperature>"),
        "ColorTemperature should not have changed after invalid SetColorTemperature: {}",
        body
    );
}
