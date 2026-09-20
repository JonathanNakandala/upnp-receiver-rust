//! Exercises `upnp-suite-client` against a real, running renderer over
//! HTTP (not in-process trait calls) - the first real test that the new
//! client crate actually speaks this renderer's SOAP dialect correctly,
//! independent of the video window menu that will eventually use it.

mod common;

use common::TestServer;
use upnp_suite_client::RendererClient;

async fn connect(server: &TestServer) -> RendererClient {
    let description_url = format!("http://{}/description.xml", server.address);
    RendererClient::connect(&description_url)
        .await
        .expect("failed to connect to the test renderer")
}

#[tokio::test]
async fn play_pause_stop_round_trip_through_soap() {
    let server = TestServer::new().await;
    assert!(server.wait_for_ready().await);
    let client = connect(&server).await;
    let av_transport = client.av_transport().expect("no AVTransport service");

    // No media loaded - fallback AVTransportImpl rejects Play from
    // NO_MEDIA_PRESENT, so this should surface as a SOAP fault rather than
    // silently succeeding.
    assert!(av_transport.play().await.is_err());

    let info = av_transport
        .get_transport_info()
        .await
        .expect("GetTransportInfo failed");
    assert_eq!(info.current_transport_state, "NO_MEDIA_PRESENT");
}

#[tokio::test]
async fn volume_round_trips_through_soap() {
    let server = TestServer::new().await;
    assert!(server.wait_for_ready().await);
    let client = connect(&server).await;
    let rendering_control = client
        .rendering_control()
        .expect("no RenderingControl service");

    assert_eq!(rendering_control.get_volume().await.unwrap(), 50);

    rendering_control.set_volume(80).await.unwrap();
    assert_eq!(rendering_control.get_volume().await.unwrap(), 80);

    assert!(!rendering_control.get_mute().await.unwrap());
    rendering_control.set_mute(true).await.unwrap();
    assert!(rendering_control.get_mute().await.unwrap());
}
