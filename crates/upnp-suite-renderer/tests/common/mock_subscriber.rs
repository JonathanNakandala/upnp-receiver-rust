//! A minimal HTTP server standing in for a UPnP control point's own embedded
//! server, which is what GENA actually delivers NOTIFY requests to. Hand-
//! rolled rather than pulling in a web framework for tests: it only needs to
//! read one simple, always-Content-Length-terminated request at a time and
//! hand back a bare 200, matching what real control points expect.

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

pub struct Notification {
    pub sid: String,
    pub seq: u32,
    pub body: String,
}

pub struct MockSubscriber {
    pub callback_url: String,
    receiver: mpsc::UnboundedReceiver<Notification>,
}

impl MockSubscriber {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Some(notification) = read_notify(socket).await {
                        let _ = tx.send(notification);
                    }
                });
            }
        });

        Self {
            callback_url: format!("http://{}/callback", addr),
            receiver: rx,
        }
    }

    /// Waits for the next NOTIFY this subscriber has received, panicking if
    /// none arrives within a few seconds.
    pub async fn next_notification(&mut self) -> Notification {
        tokio::time::timeout(Duration::from_secs(5), self.receiver.recv())
            .await
            .expect("timed out waiting for a NOTIFY")
            .expect("mock subscriber channel closed")
    }

    /// Asserts no NOTIFY arrives within `duration` — used to prove
    /// unsubscribing actually stops delivery.
    pub async fn expect_no_notification(&mut self, duration: Duration) {
        match tokio::time::timeout(duration, self.receiver.recv()).await {
            Ok(Some(n)) => panic!(
                "expected no NOTIFY, but got one (sid={}, seq={})",
                n.sid, n.seq
            ),
            Ok(None) => panic!("mock subscriber channel closed"),
            Err(_) => {} // timed out waiting — exactly what we want
        }
    }
}

async fn read_notify(mut socket: TcpStream) -> Option<Notification> {
    let (read_half, mut write_half) = socket.split();
    let mut reader = BufReader::new(read_half);

    let mut request_line = String::new();
    reader.read_line(&mut request_line).await.ok()?;

    let mut sid = None;
    let mut seq = None;
    let mut content_length = 0usize;

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await.ok()?;
        let trimmed = line.trim_end();
        if n == 0 || trimmed.is_empty() {
            break;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("sid:") {
            sid = Some(trimmed[4..].trim().to_string());
        } else if let Some(v) = lower.strip_prefix("seq:") {
            seq = v.trim().parse().ok();
        } else if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await.ok()?;

    write_half
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
        .await
        .ok()?;

    Some(Notification {
        sid: sid?,
        seq: seq?,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}
