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

// HorizontalKeystone specifically covers the one thing new relative to
// every other picture control so far: a *signed*, negative-capable range
// (validate_signed_range, not validate_range) - out-of-range on both ends,
// not just the top.

#[test(tokio::test)]
async fn test_get_horizontal_keystone_default() {
    let (_server, client) = setup().await;

    let resp = client.get_horizontal_keystone().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentHorizontalKeystone>0</CurrentHorizontalKeystone>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_horizontal_keystone_changes_get_horizontal_keystone() {
    let (_server, client) = setup().await;

    let resp = client.set_horizontal_keystone(-25).await;
    assert_response(resp, 200).await;

    let resp = client.get_horizontal_keystone().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentHorizontalKeystone>-25</CurrentHorizontalKeystone>"),
        "HorizontalKeystone change was not reflected: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_horizontal_keystone_rejects_out_of_range() {
    let (_server, client) = setup().await;

    let resp = client.set_horizontal_keystone(-51).await;
    assert_response(resp, 500).await;
    let resp = client.set_horizontal_keystone(51).await;
    assert_response(resp, 500).await;

    let resp = client.get_horizontal_keystone().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentHorizontalKeystone>0</CurrentHorizontalKeystone>"),
        "HorizontalKeystone should not have changed after invalid SetHorizontalKeystone: {}",
        body
    );
}

// Sharpness specifically covers the one thing new relative to every other
// picture control so far: neutral sits at the range's own minimum (0), not
// its midpoint - there's no "soften" concept, only "how much sharpening".

#[test(tokio::test)]
async fn test_get_sharpness_default() {
    let (_server, client) = setup().await;

    let resp = client.get_sharpness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentSharpness>0</CurrentSharpness>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_sharpness_changes_get_sharpness() {
    let (_server, client) = setup().await;

    let resp = client.set_sharpness(75).await;
    assert_response(resp, 200).await;

    let resp = client.get_sharpness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentSharpness>75</CurrentSharpness>"),
        "Sharpness change was not reflected: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_sharpness_rejects_out_of_range() {
    let (_server, client) = setup().await;

    let resp = client.set_sharpness(101).await;
    assert_response(resp, 500).await;

    let resp = client.get_sharpness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentSharpness>0</CurrentSharpness>"),
        "Sharpness should not have changed after invalid SetSharpness: {}",
        body
    );
}

// Loudness covers what's new relative to every picture control above: it's
// the one audio (not video) RenderingControl field this renderer implements
// besides Volume/Mute, and - like Mute - it's a boolean carrying a `Channel`
// argument, not a ranged value.

#[test(tokio::test)]
async fn test_get_loudness_default() {
    let (_server, client) = setup().await;

    let resp = client.get_loudness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentLoudness>false</CurrentLoudness>")
            || body.contains("<CurrentLoudness>0</CurrentLoudness>"),
        "Unexpected body: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_loudness_changes_get_loudness() {
    let (_server, client) = setup().await;

    let resp = client.set_loudness(true).await;
    assert_response(resp, 200).await;

    let resp = client.get_loudness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentLoudness>true</CurrentLoudness>")
            || body.contains("<CurrentLoudness>1</CurrentLoudness>"),
        "Loudness change was not reflected: {}",
        body
    );

    // Toggle back
    let resp = client.set_loudness(false).await;
    assert_response(resp, 200).await;

    let resp = client.get_loudness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentLoudness>false</CurrentLoudness>")
            || body.contains("<CurrentLoudness>0</CurrentLoudness>"),
        "Loudness change was not reflected: {}",
        body
    );
}

// GetStateVariables is a generic reflection action, not its own state - it
// just reports whatever the individual Getxxx() actions above would. The
// StateVariableValuePairs it returns is itself XML, so the response body
// carries it double-escaped (once for the inner XML's own attributes, once
// for embedding as this response field's text) - the same convention
// `LastChange` NOTIFY bodies use, see `server::eventing::last_change`.

#[test(tokio::test)]
async fn test_get_state_variables_single_name() {
    let (_server, client) = setup().await;

    let resp = client.get_state_variables("Volume").await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains(r#"variableName="Volume" channel="Master""#),
        "Unexpected body: {}",
        body
    );
    assert!(
        body.contains("&gt;50&lt;/stateVariable&gt;"),
        "Expected default Volume (50) in the reported value: {}",
        body
    );
    // Brightness wasn't asked for, so it shouldn't be reported.
    assert!(
        !body.contains(r#"variableName="Brightness""#),
        "Unrequested variable leaked into the response: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_get_state_variables_csv_list() {
    let (_server, client) = setup().await;

    let resp = client.get_state_variables("Volume,Loudness").await;
    let body = assert_response(resp, 200).await;
    assert!(body.contains(r#"variableName="Volume""#), "{}", body);
    assert!(body.contains(r#"variableName="Loudness""#), "{}", body);
}

#[test(tokio::test)]
async fn test_get_state_variables_wildcard_reports_everything() {
    let (_server, client) = setup().await;

    let resp = client.get_state_variables("*").await;
    let body = assert_response(resp, 200).await;
    for name in [
        "Volume",
        "VolumeDB",
        "Mute",
        "Brightness",
        "Contrast",
        "RedVideoGain",
        "GreenVideoGain",
        "BlueVideoGain",
        "RedVideoBlackLevel",
        "GreenVideoBlackLevel",
        "BlueVideoBlackLevel",
        "ColorTemperature",
        "HorizontalKeystone",
        "VerticalKeystone",
        "Sharpness",
        "Loudness",
    ] {
        assert!(
            body.contains(&format!(r#"variableName="{name}""#)),
            "Missing {name} in wildcard response: {}",
            body
        );
    }
}

#[test(tokio::test)]
async fn test_get_state_variables_rejects_unknown_name() {
    let (_server, client) = setup().await;

    let resp = client.get_state_variables("NotARealVariable").await;
    assert_response(resp, 400).await;
}

// Annex B.21 "legacy compatible transforms" - GetAllowedTransforms/
// GetTransforms/SetTransforms/GetAllAvailableTransforms alias the actions
// above under named `transform` elements instead. Only covers what's
// distinct from GetStateVariables above: transform naming (the `_Master`
// suffix on Volume/VolumeDB/Mute/Loudness), that SetTransforms actually
// changes state, and that GetAllAvailableTransforms takes no InstanceID.

#[test(tokio::test)]
async fn test_get_allowed_transforms_lists_every_transform() {
    let (_server, client) = setup().await;

    let resp = client.get_allowed_transforms().await;
    let body = assert_response(resp, 200).await;
    for name in ["Volume_Master", "VolumeDB_Master", "Mute_Master", "Loudness_Master", "Brightness"] {
        assert!(
            body.contains(&format!(r#"name="{name}""#)),
            "Missing {name} in GetAllowedTransforms response: {}",
            body
        );
    }
}

#[test(tokio::test)]
async fn test_get_all_available_transforms_matches_get_allowed_transforms() {
    let (_server, client) = setup().await;

    let allowed = assert_response(client.get_allowed_transforms().await, 200).await;
    let all_available = assert_response(client.get_all_available_transforms().await, 200).await;
    assert!(all_available.contains("Volume_Master"), "{}", all_available);
    // Same catalog either way - this renderer doesn't restrict transforms by
    // InstanceID/content type.
    assert!(allowed.contains("Volume_Master"), "{}", allowed);
}

#[test(tokio::test)]
async fn test_get_transforms_reports_default_volume() {
    let (_server, client) = setup().await;

    let resp = client.get_transforms().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains(r#"name="Volume_Master""#) && body.contains("&gt;50&lt;/value&gt;"),
        "Expected default Volume (50) under Volume_Master: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_transforms_changes_volume_and_get_volume() {
    let (_server, client) = setup().await;

    let resp = client
        .set_transforms(
            r#"<TransformSettings xmlns="urn:schemas-upnp-org:av:TransformSettings"><transform name="Volume_Master"><value>77</value></transform></TransformSettings>"#,
        )
        .await;
    assert_response(resp, 200).await;

    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>77</CurrentVolume>"),
        "SetTransforms(Volume_Master) was not reflected in GetVolume: {}",
        body
    );

    let resp = client.get_transforms().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("&gt;77&lt;/value&gt;"),
        "SetTransforms(Volume_Master) was not reflected in GetTransforms: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_transforms_rejects_unknown_transform() {
    let (_server, client) = setup().await;

    let resp = client
        .set_transforms(
            r#"<TransformSettings xmlns="urn:schemas-upnp-org:av:TransformSettings"><transform name="NotARealTransform"><value>1</value></transform></TransformSettings>"#,
        )
        .await;
    assert_response(resp, 400).await;
}

#[test(tokio::test)]
async fn test_set_transforms_rejects_out_of_range_value() {
    let (_server, client) = setup().await;

    let resp = client
        .set_transforms(
            r#"<TransformSettings xmlns="urn:schemas-upnp-org:av:TransformSettings"><transform name="Volume_Master"><value>500</value></transform></TransformSettings>"#,
        )
        .await;
    assert_response(resp, 400).await;

    // State should be unchanged after a rejected update.
    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>50</CurrentVolume>"),
        "Volume should not have changed after invalid SetTransforms: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_transforms_applies_whichever_transforms_succeed() {
    let (_server, client) = setup().await;

    // One good transform, one unknown - per 5.4.41 the action succeeds as
    // long as at least one requested transform applies.
    let resp = client
        .set_transforms(
            r#"<TransformSettings xmlns="urn:schemas-upnp-org:av:TransformSettings"><transform name="Brightness"><value>80</value></transform><transform name="NotARealTransform"><value>1</value></transform></TransformSettings>"#,
        )
        .await;
    assert_response(resp, 200).await;

    let resp = client.get_brightness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentBrightness>80</CurrentBrightness>"),
        "The valid transform in a mixed request should still have applied: {}",
        body
    );
}

// SetStateVariables is GetStateVariables' write side - same
// stateVariableValuePairs dialect, but it also gates on a
// RenderingControlUDN/ServiceType/ServiceId compatibility check before
// applying anything (5.4.38), unlike every other action in this file.

const SERVICE_TYPE: &str = "urn:schemas-upnp-org:service:RenderingControl:1";
const SERVICE_ID: &str = "urn:upnp-org:serviceId:RenderingControl";

#[test(tokio::test)]
async fn test_set_state_variables_changes_volume_and_brightness() {
    let (_server, client) = setup().await;
    let udn = client.device_udn().await;

    let resp = client
        .set_state_variables(
            &udn,
            SERVICE_TYPE,
            SERVICE_ID,
            r#"<stateVariableValuePairs xmlns="urn:schemas-upnp-org:av:avs"><stateVariable variableName="Volume">65</stateVariable><stateVariable variableName="Brightness">30</stateVariable></stateVariableValuePairs>"#,
        )
        .await;
    let body = assert_response(resp, 200).await;
    assert!(body.contains("Volume"), "{}", body);
    assert!(body.contains("Brightness"), "{}", body);

    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(body.contains("<CurrentVolume>65</CurrentVolume>"), "{}", body);

    let resp = client.get_brightness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentBrightness>30</CurrentBrightness>"),
        "{}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_state_variables_rejects_wrong_udn() {
    let (_server, client) = setup().await;

    let resp = client
        .set_state_variables(
            "uuid:not-the-real-device",
            SERVICE_TYPE,
            SERVICE_ID,
            r#"<stateVariableValuePairs xmlns="urn:schemas-upnp-org:av:avs"><stateVariable variableName="Volume">65</stateVariable></stateVariableValuePairs>"#,
        )
        .await;
    assert_response(resp, 400).await;

    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>50</CurrentVolume>"),
        "Volume should not have changed when the UDN check fails: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_state_variables_rejects_wrong_service_type() {
    let (_server, client) = setup().await;
    let udn = client.device_udn().await;

    let resp = client
        .set_state_variables(
            &udn,
            "urn:schemas-upnp-org:service:RenderingControl:99",
            SERVICE_ID,
            r#"<stateVariableValuePairs xmlns="urn:schemas-upnp-org:av:avs"><stateVariable variableName="Volume">65</stateVariable></stateVariableValuePairs>"#,
        )
        .await;
    assert_response(resp, 400).await;
}

#[test(tokio::test)]
async fn test_set_state_variables_rejects_forbidden_variable() {
    let (_server, client) = setup().await;
    let udn = client.device_udn().await;

    // LastChange must never be settable - see the exclusion list both
    // GetStateVariables and SetStateVariables share.
    let resp = client
        .set_state_variables(
            &udn,
            SERVICE_TYPE,
            SERVICE_ID,
            r#"<stateVariableValuePairs xmlns="urn:schemas-upnp-org:av:avs"><stateVariable variableName="LastChange">bogus</stateVariable></stateVariableValuePairs>"#,
        )
        .await;
    assert_response(resp, 400).await;
}

#[test(tokio::test)]
async fn test_set_state_variables_rejects_out_of_range_value() {
    let (_server, client) = setup().await;
    let udn = client.device_udn().await;

    let resp = client
        .set_state_variables(
            &udn,
            SERVICE_TYPE,
            SERVICE_ID,
            r#"<stateVariableValuePairs xmlns="urn:schemas-upnp-org:av:avs"><stateVariable variableName="Volume">500</stateVariable></stateVariableValuePairs>"#,
        )
        .await;
    assert_response(resp, 400).await;

    let resp = client.get_volume().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentVolume>50</CurrentVolume>"),
        "Volume should not have changed after a rejected SetStateVariables: {}",
        body
    );
}

#[test(tokio::test)]
async fn test_set_state_variables_is_all_or_nothing() {
    let (_server, client) = setup().await;
    let udn = client.device_udn().await;

    // Brightness is valid, LastChange is forbidden - unlike SetTransforms,
    // nothing here should apply since the request as a whole is rejected.
    let resp = client
        .set_state_variables(
            &udn,
            SERVICE_TYPE,
            SERVICE_ID,
            r#"<stateVariableValuePairs xmlns="urn:schemas-upnp-org:av:avs"><stateVariable variableName="Brightness">90</stateVariable><stateVariable variableName="LastChange">bogus</stateVariable></stateVariableValuePairs>"#,
        )
        .await;
    assert_response(resp, 400).await;

    let resp = client.get_brightness().await;
    let body = assert_response(resp, 200).await;
    assert!(
        body.contains("<CurrentBrightness>50</CurrentBrightness>"),
        "Brightness should not have changed - the whole request should have been rejected: {}",
        body
    );
}
