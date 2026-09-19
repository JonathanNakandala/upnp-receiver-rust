use std::path::{Path, PathBuf};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tracing::warn;

/// A single captured SOAP request/response pair, destined for a per-client log file.
///
/// This exists purely as a debugging/protocol-discovery aid: pointing a real
/// control point (BubbleUPnP, VLC, a smart speaker, ...) at this renderer with
/// `--log-clients` on lets us see exactly what each client sends, and which of
/// those requests we don't yet handle.
pub struct ClientLogEntry {
    pub remote_addr: String,
    pub user_agent: Option<String>,
    pub method: String,
    pub path: String,
    pub soap_action: Option<String>,
    /// GENA subscription headers (CALLBACK, NT, SID, TIMEOUT, ...) present on
    /// a SUBSCRIBE/UNSUBSCRIBE request — empty for ordinary SOAP calls, which
    /// carry everything interesting in the body instead.
    pub gena_headers: Vec<(String, String)>,
    pub request_body: String,
    pub response_status: u16,
    pub response_body: String,
}

pub struct ClientLogger {
    base_dir: PathBuf,
}

impl ClientLogger {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Groups a client's log by its own identity (User-Agent when the client sends
    /// one, otherwise its IP) rather than by connection, so repeated requests from
    /// the same piece of client software accumulate in one place across restarts.
    fn client_dir_name(entry: &ClientLogEntry) -> String {
        match entry.user_agent.as_deref().map(str::trim) {
            Some(ua) if !ua.is_empty() => sanitize(ua),
            _ => format!("ip-{}", sanitize(&entry.remote_addr)),
        }
    }

    pub async fn log(&self, entry: &ClientLogEntry) {
        if let Err(e) = self.try_log(entry).await {
            warn!(error = %e, "Failed to write client request log");
        }
    }

    async fn try_log(&self, entry: &ClientLogEntry) -> std::io::Result<()> {
        let client_dir = self.base_dir.join(Self::client_dir_name(entry));
        fs::create_dir_all(&client_dir).await?;

        let block = format_block(entry);

        append(&client_dir.join("requests.log"), &block).await?;

        // Separate, smaller file so "what's missing" can be scanned without
        // wading through every successful call.
        if entry.response_status >= 400 {
            append(&client_dir.join("unimplemented.log"), &block).await?;
        }

        Ok(())
    }
}

async fn append(path: &Path, content: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(content.as_bytes()).await
}

fn format_block(entry: &ClientLogEntry) -> String {
    let gena_headers = entry
        .gena_headers
        .iter()
        .map(|(name, value)| format!("{}: {}\n", name, value))
        .collect::<String>();

    format!(
        "=== {} ===\n\
         Remote: {}\n\
         User-Agent: {}\n\
         Request: {} {}\n\
         SOAPAction: {}\n\
         {}\
         \n\
         {}\n\
         \n\
         --- Response ({}) ---\n\
         {}\n\
         \n",
        httpdate::fmt_http_date(std::time::SystemTime::now()),
        entry.remote_addr,
        entry.user_agent.as_deref().unwrap_or("(none)"),
        entry.method,
        entry.path,
        entry.soap_action.as_deref().unwrap_or("(none)"),
        gena_headers,
        entry.request_body.trim(),
        entry.response_status,
        entry.response_body.trim(),
    )
}

/// Turns a User-Agent (or address) string into a safe, readable directory name:
/// lowercase, alphanumerics kept, everything else collapsed to single `-`.
fn sanitize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = out.trim_matches('-');
    let truncated: String = trimmed.chars().take(80).collect();

    if truncated.is_empty() {
        "unknown-client".to_string()
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_common_user_agents() {
        assert_eq!(sanitize("BubbleUPnP/3.4 (Linux;Android)"), "bubbleupnp-3.4-linux-android");
        assert_eq!(sanitize("VLC/3.0.20 LibVLC/3.0.20"), "vlc-3.0.20-libvlc-3.0.20");
        assert_eq!(sanitize(""), "unknown-client");
        assert_eq!(sanitize("   "), "unknown-client");
    }

    #[test]
    fn falls_back_to_ip_when_no_user_agent() {
        let entry = ClientLogEntry {
            remote_addr: "192.168.1.50:51322".to_string(),
            user_agent: None,
            method: "POST".to_string(),
            path: "/services/AVTransport/control".to_string(),
            soap_action: None,
            gena_headers: Vec::new(),
            request_body: String::new(),
            response_status: 200,
            response_body: String::new(),
        };
        assert_eq!(ClientLogger::client_dir_name(&entry), "ip-192.168.1.50-51322");
    }
}
