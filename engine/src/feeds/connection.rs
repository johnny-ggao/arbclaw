use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::{client_async, WebSocketStream, MaybeTlsStream};
use tracing::info;
use url::Url;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn get_proxy() -> Option<(String, u16)> {
    if let Ok(proxy) = std::env::var("https_proxy").or_else(|_| std::env::var("HTTPS_PROXY")) {
        if let Ok(url) = Url::parse(&proxy) {
            let host = url.host_str().unwrap_or("127.0.0.1").to_string();
            let port = url.port().unwrap_or(7890);
            return Some((host, port));
        }
    }
    None
}

pub async fn connect_ws(ws_url: &str) -> Result<WsStream> {
    let url = Url::parse(ws_url)?;
    let host = url.host_str().ok_or_else(|| anyhow::anyhow!("no host"))?.to_string();
    let port = url.port().unwrap_or(443);

    if let Some((proxy_host, proxy_port)) = get_proxy() {
        info!("connecting via proxy {proxy_host}:{proxy_port} to {host}:{port}");
        connect_via_proxy(ws_url, &host, port, &proxy_host, proxy_port).await
    } else {
        connect_direct(ws_url).await
    }
}

async fn connect_direct(ws_url: &str) -> Result<WsStream> {
    let (ws, _) = tokio_tungstenite::connect_async(ws_url).await?;
    Ok(ws)
}

async fn connect_via_proxy(
    ws_url: &str,
    target_host: &str,
    target_port: u16,
    proxy_host: &str,
    proxy_port: u16,
) -> Result<WsStream> {
    // Connect to proxy
    let mut tcp = TcpStream::connect(format!("{proxy_host}:{proxy_port}")).await?;

    // CONNECT tunnel through HTTP proxy
    async_http_proxy::http_connect_tokio(&mut tcp, target_host, target_port).await
        .map_err(|e| anyhow::anyhow!("proxy CONNECT failed: {e}"))?;

    // TLS handshake over the tunnel
    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates({
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            roots
        })
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(tls_config));
    let domain = rustls::pki_types::ServerName::try_from(target_host.to_string())?;
    let tls_stream = connector.connect(domain, tcp).await?;

    // WebSocket handshake over TLS
    let request = Request::builder()
        .uri(ws_url)
        .header("Host", target_host)
        .header("Sec-WebSocket-Key", generate_key())
        .header("Sec-WebSocket-Version", "13")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .body(())?;

    let (ws, _) = client_async(request, MaybeTlsStream::Rustls(tls_stream)).await?;
    Ok(ws)
}
