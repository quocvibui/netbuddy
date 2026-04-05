//! MITM HTTP/S proxy — intercepts browser traffic via hudsucker.
//!
//! Captures text/html and application/json responses, extracts the body
//! (up to 200 KB), truncates to 2000 chars, and sends a PageEntry
//! downstream to the store ingestion task.
//!
//! TLS interception uses a self-signed CA generated on first run and
//! saved to `certs/`.  The user must trust this CA in their OS keychain
//! and configure their browser to proxy through 127.0.0.1:<port>.

use std::net::SocketAddr;
use std::path::Path;

use anyhow::Result;
use http_body_util::{BodyExt, Full};
use hudsucker::{
    certificate_authority::RcgenAuthority,
    decode_response,
    hyper::{Request, Response},
    rcgen::{self, CertificateParams, Issuer, KeyPair},
    rustls::crypto::aws_lc_rs,
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse,
};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::store::PageEntry;

const CERT_PATH: &str = "certs/ca.crt";
const KEY_PATH: &str = "certs/ca.key";
const MAX_BODY_BYTES: usize = 200 * 1024;  // skip bodies larger than 200 KB
const BODY_TRUNCATE_CHARS: usize = 2000;   // store at most 2000 chars per page

/// Per-connection handler.  Captures the request URI on the way in,
/// then grabs the response body on the way out.
#[derive(Clone)]
struct ProxyHandler {
    entry_tx: mpsc::Sender<PageEntry>,
    current_uri: Option<String>,
}

impl HttpHandler for ProxyHandler {
    fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> impl std::future::Future<Output = RequestOrResponse> + Send {
        let uri = req.uri().to_string();
        let host = req
            .headers()
            .get("host")
            .and_then(|v: &hyper::header::HeaderValue| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // CONNECT requests have relative URIs; reconstruct the full URL.
        self.current_uri = if uri.starts_with("http") {
            Some(uri)
        } else {
            Some(format!("https://{host}{uri}"))
        };

        async { RequestOrResponse::Request(req) }
    }

    fn handle_response(
        &mut self,
        _ctx: &HttpContext,
        res: Response<Body>,
    ) -> impl std::future::Future<Output = Response<Body>> + Send {
        let url = self.current_uri.clone().unwrap_or_default();
        let entry_tx = self.entry_tx.clone();

        async move {
            // Only capture text content we can actually read.
            let content_type = res
                .headers()
                .get("content-type")
                .and_then(|v: &hyper::header::HeaderValue| v.to_str().ok())
                .unwrap_or("")
                .to_lowercase();

            let is_html = content_type.contains("text/html");
            let is_json = content_type.contains("application/json");

            if !is_html && !is_json {
                return res;
            }

            // decode_response handles gzip/brotli/deflate decompression.
            let res = match decode_response(res) {
                Ok(r) => r,
                Err(e) => {
                    warn!("decode failed for {url}: {e}");
                    return Response::builder()
                        .status(502)
                        .body(Body::empty())
                        .unwrap();
                }
            };

            let (parts, body) = res.into_parts();
            let collected = match body.collect().await {
                Ok(c) => c,
                Err(e) => {
                    warn!("body read failed for {url}: {e}");
                    return Response::from_parts(parts, Body::empty());
                }
            };

            let bytes = collected.to_bytes();

            // Don't store huge payloads (e.g. large JSON API dumps).
            if bytes.len() > MAX_BODY_BYTES {
                return Response::from_parts(parts, Body::from(Full::new(bytes)));
            }

            let body_str = String::from_utf8_lossy(&bytes);
            let truncated: String = body_str.chars().take(BODY_TRUNCATE_CHARS).collect();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let entry = PageEntry {
                url,
                body: truncated,
                timestamp,
            };

            // Non-blocking send; if the channel is full we drop the entry
            // rather than stalling the proxy pipeline.
            if let Err(e) = entry_tx.try_send(entry) {
                warn!("failed to send page entry: {e}");
            }

            Response::from_parts(parts, Body::from(Full::new(bytes)))
        }
    }
}

/// Load existing CA cert/key from disk, or generate a new self-signed pair.
fn load_or_generate_ca() -> Result<Issuer<'static, KeyPair>> {
    let cert_path = Path::new(CERT_PATH);
    let key_path = Path::new(KEY_PATH);

    if cert_path.exists() && key_path.exists() {
        info!("loading existing CA cert and key");
        let key_pem = std::fs::read_to_string(key_path)?;
        let cert_pem = std::fs::read_to_string(cert_path)?;
        let key_pair = KeyPair::from_pem(&key_pem)?;
        let issuer = Issuer::from_ca_cert_pem(&cert_pem, key_pair)?;
        return Ok(issuer);
    }

    info!("generating new CA certificate");
    let mut params = CertificateParams::default();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "netmind CA");
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "netmind");

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    std::fs::create_dir_all("certs")?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();
    std::fs::write(cert_path, &cert_pem)?;
    std::fs::write(key_path, &key_pem)?;
    info!("CA cert saved to {CERT_PATH}, key saved to {KEY_PATH}");

    let issuer = Issuer::from_ca_cert_pem(&cert_pem, key_pair)?;
    Ok(issuer)
}

/// Ports to try, in order.  We pick the first one that isn't occupied.
const PREFERRED_PORTS: &[u16] = &[8080, 8081, 8082, 9080, 9090];

/// Start the MITM proxy.  Blocks until shutdown (ctrl-c).
/// Updates `state` with the bound port before entering the accept loop.
pub async fn start_proxy(
    entry_tx: mpsc::Sender<PageEntry>,
    state: crate::state::SharedState,
) -> Result<()> {
    let issuer = load_or_generate_ca()?;

    let handler = ProxyHandler {
        entry_tx,
        current_uri: None,
    };

    let mut bound_port = None;
    for &port in PREFERRED_PORTS {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        if std::net::TcpListener::bind(addr).is_ok() {
            bound_port = Some(port);
            break;
        }
        warn!("port {port} is taken, trying next...");
    }
    let port = bound_port.ok_or_else(|| anyhow::anyhow!(
        "all proxy ports are taken ({PREFERRED_PORTS:?})"
    ))?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let ca = RcgenAuthority::new(issuer, 1_000, aws_lc_rs::default_provider());

    {
        let mut st = state.lock().unwrap();
        st.proxy_port = port;
        st.proxy_active = true;
    }
    info!("starting MITM proxy on {addr}");

    let proxy = Proxy::builder()
        .with_addr(addr)
        .with_ca(ca)
        .with_rustls_connector(aws_lc_rs::default_provider())
        .with_http_handler(handler)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .build()?;

    proxy.start().await?;
    info!("proxy shut down");
    {
        let mut st = state.lock().unwrap();
        st.proxy_active = false;
    }
    Ok(())
}
