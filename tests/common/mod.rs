extern crate upnp_receiver;
pub mod mock_subscriber;
pub mod soap_client;

pub use mock_subscriber::MockSubscriber;
pub use soap_client::SoapClient;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;
use upnp_receiver::server::ClientLogger;
use upnp_receiver::services::MediaRenderer;
use upnp_receiver::{HttpServer, MediaRendererConfig};
pub struct TestServer {
    pub address: SocketAddr,
    shutdown_tx: oneshot::Sender<()>,
}

impl TestServer {
    pub async fn new() -> Self {
        Self::start(None).await
    }

    /// Like [`Self::new`], but with `--log-clients`-style logging enabled —
    /// for tests that need to verify what actually gets written to a
    /// client's log file (e.g. outbound GENA NOTIFYs).
    pub async fn new_with_client_logger(logger: Arc<ClientLogger>) -> Self {
        Self::start(Some(logger)).await
    }

    async fn start(client_logger: Option<Arc<ClientLogger>>) -> Self {
        tracing::debug!("Creating new test server");

        let renderer = MediaRenderer::new(
            MediaRendererConfig::builder()
                .port(0)
                .friendly_name("Test Renderer".to_string())
                .build(),
        )
            .unwrap();

        tracing::debug!("Created media renderer");

        let http_server =
            HttpServer::new(Arc::new(renderer)).with_client_logger(client_logger);
        let bind_addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Create and bind the TCP listener
        let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();
        let addr = listener.local_addr().unwrap();

        tracing::info!(address = %addr, "Server bound to address");

        // Let the HttpServer create its own Router with proper routes.
        // `into_make_service_with_connect_info` is needed to match production
        // (src/server/http_server.rs) — handlers that extract `ConnectInfo`
        // (e.g. the GENA SUBSCRIBE endpoint) fail without it.
        tokio::spawn(async move {
            tracing::info!("Starting server");
            axum::serve(
                listener,
                http_server
                    .router()
                    .into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
                tracing::info!("Server shutting down");
            })
            .await
            .unwrap();
        });

        Self {
            address: addr,
            shutdown_tx,
        }
    }

    pub async fn wait_for_ready(&self) -> bool {
        tracing::debug!("Waiting for server to be ready");
        // Attempt to connect and check server is ready
        for i in 0..5 {
            if reqwest::get(format!("http://{}/description.xml", self.address))
                .await
                .is_ok()
            {
                tracing::info!("Server is ready");
                return true;
            }
            tracing::debug!(attempt = i + 1, "Server not ready, retrying");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        tracing::warn!("Server failed to become ready");
        false
    }
}
