mod common;
use common::{MockSubscriber, SoapClient, TestServer};

use reqwest::Method;
use std::time::Duration;

async fn setup() -> (TestServer, SoapClient) {
    let server = TestServer::new().await;
    assert!(
        server.wait_for_ready().await,
        "Server failed to become ready"
    );
    let client = SoapClient::new(server.address);
    (server, client)
}

fn subscribe_method() -> Method {
    Method::from_bytes(b"SUBSCRIBE").unwrap()
}

fn unsubscribe_method() -> Method {
    Method::from_bytes(b"UNSUBSCRIBE").unwrap()
}

#[tokio::test]
async fn full_gena_loop_for_av_transport() {
    let (server, client) = setup().await;
    let mut subscriber = MockSubscriber::start().await;
    let http = reqwest::Client::new();
    let events_url = format!("http://{}/services/AVTransport/events", server.address);

    // --- SUBSCRIBE ---
    let response = http
        .request(subscribe_method(), &events_url)
        .header("CALLBACK", format!("<{}>", subscriber.callback_url))
        .header("NT", "upnp:event")
        .header("TIMEOUT", "Second-120")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let sid = response
        .headers()
        .get("SID")
        .expect("SUBSCRIBE response must carry a SID header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(sid.starts_with("uuid:"));
    assert_eq!(
        response.headers().get("TIMEOUT").unwrap(),
        "Second-120",
        "requested timeout should be honored when within bounds"
    );

    // --- Mandatory initial event (SEQ 0), reflecting current (no media) state ---
    let initial = subscriber.next_notification().await;
    assert_eq!(initial.sid, sid);
    assert_eq!(initial.seq, 0);
    assert!(
        initial.body.contains("NO_MEDIA_PRESENT"),
        "initial event should reflect the pre-load state, got: {}",
        initial.body
    );

    // --- Trigger a real transport state change ---
    let resp = client
        .set_av_transport_uri("http://example.com/test.mp3", None)
        .await;
    assert_eq!(resp.status(), 200);

    let after_load = subscriber.next_notification().await;
    assert_eq!(after_load.sid, sid);
    assert_eq!(after_load.seq, 1);
    assert!(after_load.body.contains("STOPPED"));

    let resp = client.play().await;
    assert_eq!(resp.status(), 200);

    let after_play = subscriber.next_notification().await;
    assert_eq!(after_play.sid, sid);
    assert_eq!(after_play.seq, 2);
    assert!(
        after_play.body.contains("PLAYING"),
        "expected a PLAYING event after Play, got: {}",
        after_play.body
    );

    // --- UNSUBSCRIBE ---
    let response = http
        .request(unsubscribe_method(), &events_url)
        .header("SID", &sid)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Further state changes must not reach the now-unsubscribed callback.
    let resp = client.pause().await;
    assert_eq!(resp.status(), 200);
    subscriber
        .expect_no_notification(Duration::from_millis(500))
        .await;
}

#[tokio::test]
async fn full_gena_loop_for_rendering_control() {
    let (server, client) = setup().await;
    let mut subscriber = MockSubscriber::start().await;
    let http = reqwest::Client::new();
    let events_url = format!("http://{}/services/RenderingControl/events", server.address);

    let response = http
        .request(subscribe_method(), &events_url)
        .header("CALLBACK", format!("<{}>", subscriber.callback_url))
        .header("NT", "upnp:event")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let sid = response
        .headers()
        .get("SID")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let initial = subscriber.next_notification().await;
    assert_eq!(initial.sid, sid);
    assert_eq!(initial.seq, 0);
    assert!(initial.body.contains("Volume"));
    assert!(initial.body.contains("Mute"));

    let resp = client.set_volume(80).await;
    assert_eq!(resp.status(), 200);

    let after_volume = subscriber.next_notification().await;
    assert_eq!(after_volume.sid, sid);
    assert_eq!(after_volume.seq, 1);
    assert!(
        after_volume.body.contains("val=&quot;80&quot;"),
        "expected the new volume in the event body, got: {}",
        after_volume.body
    );
}

#[tokio::test]
async fn renew_extends_the_subscription_and_unknown_sid_is_rejected() {
    let (server, _client) = setup().await;
    let subscriber = MockSubscriber::start().await;
    let http = reqwest::Client::new();
    let events_url = format!("http://{}/services/AVTransport/events", server.address);

    let response = http
        .request(subscribe_method(), &events_url)
        .header("CALLBACK", format!("<{}>", subscriber.callback_url))
        .header("NT", "upnp:event")
        .send()
        .await
        .unwrap();
    let sid = response
        .headers()
        .get("SID")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Renewal: SID header present, no CALLBACK.
    let response = http
        .request(subscribe_method(), &events_url)
        .header("SID", &sid)
        .header("TIMEOUT", "Second-60")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers().get("TIMEOUT").unwrap(), "Second-60");

    // An unknown SID must be rejected, not silently accepted.
    let response = http
        .request(subscribe_method(), &events_url)
        .header("SID", "uuid:00000000-0000-0000-0000-000000000000")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 412);

    let response = http
        .request(unsubscribe_method(), &events_url)
        .header("SID", "uuid:00000000-0000-0000-0000-000000000000")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 412);
}

#[tokio::test]
async fn subscribe_without_a_callback_is_rejected() {
    let (server, _client) = setup().await;
    let http = reqwest::Client::new();
    let events_url = format!("http://{}/services/AVTransport/events", server.address);

    let response = http
        .request(subscribe_method(), &events_url)
        .header("NT", "upnp:event")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 412);
}
