//! In-process TLS reverse proxy.
//!
//! Replaces Caddy with a Rust HTTPS server using rustls + hyper.
//! Manages a local CA, mints per-hostname leaf certs on-the-fly,
//! and forwards requests (including WebSocket upgrades) to upstream ports.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::server::ResolvesServerCert;
use rustls::sign::CertifiedKey;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::RwLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

// ---------------------------------------------------------------------------
// TlsManager — local CA creation, persistence, keychain trust, cert minting
// ---------------------------------------------------------------------------

fn tls_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".terrarium")
        .join("tls")
}

pub struct TlsManager {
    /// CA cert PEM (stored for reconstructing Issuer).
    ca_cert_pem: String,
    /// CA key pair (stored for reconstructing Issuer and signing).
    ca_key_pair: KeyPair,
    /// CA cert DER (included in leaf cert chains).
    ca_cert_der: CertificateDer<'static>,
    /// Cache of minted leaf certs by hostname.
    cert_cache: RwLock<HashMap<String, Arc<CertifiedKey>>>,
}

impl TlsManager {
    /// Load an existing CA from disk, or generate + persist + trust a new one.
    pub fn load_or_create() -> Result<Self, String> {
        let dir = tls_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create TLS dir: {}", e))?;

        let ca_cert_path = dir.join("ca.crt");
        let ca_key_path = dir.join("ca.key");

        if ca_cert_path.exists() && ca_key_path.exists() {
            Self::load_existing(&ca_cert_path, &ca_key_path)
        } else {
            Self::generate_new(&ca_cert_path, &ca_key_path)
        }
    }

    fn load_existing(cert_path: &PathBuf, key_path: &PathBuf) -> Result<Self, String> {
        let cert_pem =
            std::fs::read_to_string(cert_path).map_err(|e| format!("Read CA cert: {}", e))?;
        let key_pem =
            std::fs::read_to_string(key_path).map_err(|e| format!("Read CA key: {}", e))?;

        let key_pair =
            KeyPair::from_pem(&key_pem).map_err(|e| format!("Parse CA key: {}", e))?;

        // Validate that the PEM can be parsed as an Issuer
        let _issuer = Issuer::from_ca_cert_pem(&cert_pem, &key_pair)
            .map_err(|e| format!("Parse CA cert: {}", e))?;

        // Extract DER from PEM for inclusion in leaf cert chains
        let ca_cert_der = Self::pem_to_der(&cert_pem)?;

        eprintln!("Loaded existing Terrarium CA from {}", cert_path.display());

        Ok(Self {
            ca_cert_pem: cert_pem,
            ca_key_pair: key_pair,
            ca_cert_der,
            cert_cache: RwLock::new(HashMap::new()),
        })
    }

    fn generate_new(cert_path: &PathBuf, key_path: &PathBuf) -> Result<Self, String> {
        let key_pair = KeyPair::generate().map_err(|e| format!("Generate CA key: {}", e))?;

        let mut params = CertificateParams::new(Vec::<String>::new())
            .map_err(|e| format!("CA params: {}", e))?;
        params
            .distinguished_name
            .push(DnType::CommonName, "Terrarium Local CA");
        params
            .distinguished_name
            .push(DnType::OrganizationName, "Terrarium");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        // 10-year validity
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2034, 1, 1);

        let ca_cert = params
            .self_signed(&key_pair)
            .map_err(|e| format!("Self-sign CA: {}", e))?;

        let cert_pem = ca_cert.pem();

        // Persist PEM files
        std::fs::write(cert_path, &cert_pem)
            .map_err(|e| format!("Write CA cert: {}", e))?;
        std::fs::write(key_path, key_pair.serialize_pem())
            .map_err(|e| format!("Write CA key: {}", e))?;

        eprintln!("Generated new Terrarium CA at {}", cert_path.display());

        // Trust in macOS system keychain (requires admin — user sees a password prompt).
        // This is the same approach mkcert uses. Apps using SecureTransport (browsers,
        // native apps) will trust certs signed by this CA.
        let trust_result = std::process::Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "do shell script \"security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain '{}'\" with administrator privileges",
                    cert_path.display()
                ),
            ])
            .status();

        match trust_result {
            Ok(s) if s.success() => eprintln!("CA trusted in macOS system keychain"),
            Ok(s) => eprintln!("Keychain trust exited with {} (user may have cancelled)", s),
            Err(e) => eprintln!("Failed to run trust command: {}", e),
        }

        let ca_cert_der = CertificateDer::from(ca_cert.der().to_vec());

        Ok(Self {
            ca_cert_pem: cert_pem,
            ca_key_pair: key_pair,
            ca_cert_der,
            cert_cache: RwLock::new(HashMap::new()),
        })
    }

    /// Mint (or return cached) a leaf certificate for the given hostname.
    fn mint_cert(&self, hostname: &str) -> Result<Arc<CertifiedKey>, String> {
        // Check cache first (read lock)
        {
            let cache = self.cert_cache.read().unwrap();
            if let Some(ck) = cache.get(hostname) {
                return Ok(Arc::clone(ck));
            }
        }

        // Reconstruct Issuer from stored CA PEM + key
        let issuer = Issuer::from_ca_cert_pem(&self.ca_cert_pem, &self.ca_key_pair)
            .map_err(|e| format!("Create issuer: {}", e))?;

        // Generate leaf cert
        let leaf_key =
            KeyPair::generate().map_err(|e| format!("Generate leaf key: {}", e))?;

        let mut leaf_params = CertificateParams::new(vec![hostname.to_string()])
            .map_err(|e| format!("Leaf params: {}", e))?;
        leaf_params
            .distinguished_name
            .push(DnType::CommonName, hostname);
        leaf_params.is_ca = IsCa::NoCa;
        leaf_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        leaf_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        // macOS enforces an 825-day limit on TLS certs (even from custom CAs).
        // Use 2 years to stay well under the limit, matching mkcert's approach.
        leaf_params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        leaf_params.not_after = rcgen::date_time_ymd(2026, 1, 1);

        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &issuer)
            .map_err(|e| format!("Sign leaf cert: {}", e))?;

        let leaf_cert_der = CertificateDer::from(leaf_cert.der().to_vec());
        let leaf_key_der =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));

        let signing_key =
            rustls::crypto::aws_lc_rs::sign::any_supported_type(&leaf_key_der)
                .map_err(|e| format!("Create signing key: {}", e))?;

        let certified_key = Arc::new(CertifiedKey::new(
            vec![leaf_cert_der, self.ca_cert_der.clone()],
            signing_key,
        ));

        // Cache it
        {
            let mut cache = self.cert_cache.write().unwrap();
            cache.insert(hostname.to_string(), Arc::clone(&certified_key));
        }

        Ok(certified_key)
    }

    /// Parse PEM certificate to DER.
    fn pem_to_der(pem_str: &str) -> Result<CertificateDer<'static>, String> {
        // Find the base64 content between PEM markers
        let start_marker = "-----BEGIN CERTIFICATE-----";
        let end_marker = "-----END CERTIFICATE-----";
        let start = pem_str
            .find(start_marker)
            .ok_or("No BEGIN CERTIFICATE marker")?
            + start_marker.len();
        let end = pem_str
            .find(end_marker)
            .ok_or("No END CERTIFICATE marker")?;
        let b64: String = pem_str[start..end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        use base64::Engine;
        let der = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .map_err(|e| format!("Base64 decode: {}", e))?;
        Ok(CertificateDer::from(der))
    }
}

// ---------------------------------------------------------------------------
// TerrariumCertResolver — rustls SNI-based cert resolution
// ---------------------------------------------------------------------------

struct TerrariumCertResolver {
    tls_manager: Arc<TlsManager>,
}

impl std::fmt::Debug for TerrariumCertResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerrariumCertResolver").finish()
    }
}

impl ResolvesServerCert for TerrariumCertResolver {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<CertifiedKey>> {
        let hostname = client_hello.server_name()?;

        if !hostname.ends_with("-terrarium.local") {
            eprintln!("Proxy: rejecting SNI for non-terrarium hostname: {}", hostname);
            return None;
        }

        match self.tls_manager.mint_cert(hostname) {
            Ok(ck) => Some(ck),
            Err(e) => {
                eprintln!("Proxy: failed to mint cert for {}: {}", hostname, e);
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ProxyManager — route management + HTTPS server
// ---------------------------------------------------------------------------

pub struct ProxyManager {
    routes: Arc<RwLock<HashMap<String, u16>>>,
}

impl ProxyManager {
    /// Start the proxy server on :4443 and return the manager.
    pub async fn start(tls_manager: Arc<TlsManager>) -> Result<Self, String> {
        let routes: Arc<RwLock<HashMap<String, u16>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let resolver = Arc::new(TerrariumCertResolver {
            tls_manager: Arc::clone(&tls_manager),
        });

        let tls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(resolver);

        let acceptor = TlsAcceptor::from(Arc::new(tls_config));

        let addr = SocketAddr::from(([0, 0, 0, 0], 4443));
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| format!("Failed to bind proxy on :4443: {}", e))?;

        eprintln!("Proxy server listening on :4443");

        let routes_for_server = Arc::clone(&routes);
        tokio::spawn(async move {
            Self::accept_loop(listener, acceptor, routes_for_server).await;
        });

        Ok(Self { routes })
    }

    async fn accept_loop(
        listener: TcpListener,
        acceptor: TlsAcceptor,
        routes: Arc<RwLock<HashMap<String, u16>>>,
    ) {
        loop {
            let (stream, _addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Proxy: accept error: {}", e);
                    continue;
                }
            };

            let acceptor = acceptor.clone();
            let routes = Arc::clone(&routes);

            tokio::spawn(async move {
                let tls_stream = match acceptor.accept(stream).await {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Proxy: TLS handshake error: {}", e);
                        return;
                    }
                };

                let routes = Arc::clone(&routes);
                let conn = http1::Builder::new()
                    .serve_connection(
                        hyper_util::rt::TokioIo::new(tls_stream),
                        service_fn(move |req| {
                            let routes = Arc::clone(&routes);
                            handle_request(req, routes)
                        }),
                    )
                    .with_upgrades();

                if let Err(e) = conn.await {
                    // Connection reset by peer is normal
                    if !is_benign_error(&e) {
                        eprintln!("Proxy: connection error: {}", e);
                    }
                }
            });
        }
    }

    /// Add a route: hostname → upstream port. Pre-mints the TLS cert.
    pub fn add_route(
        &self,
        hostname: &str,
        port: u16,
        tls_manager: &TlsManager,
    ) {
        // Pre-mint cert so the first request is fast
        if let Err(e) = tls_manager.mint_cert(hostname) {
            eprintln!("Proxy: failed to pre-mint cert for {}: {}", hostname, e);
        }

        let mut routes = self.routes.write().unwrap();
        routes.insert(hostname.to_string(), port);
        eprintln!("Proxy: added route {} -> localhost:{}", hostname, port);
    }

    /// Remove a single route by hostname.
    pub fn remove_route(&self, hostname: &str) {
        let mut routes = self.routes.write().unwrap();
        routes.remove(hostname);
        eprintln!("Proxy: removed route {}", hostname);
    }

    /// Remove all routes for a project (matching `{name}-terrarium.local`).
    pub fn remove_project_routes(&self, project_name: &str) {
        let hostname = format!("{}-terrarium.local", project_name);
        let mut routes = self.routes.write().unwrap();
        routes.remove(&hostname);
        eprintln!("Proxy: removed routes for project {}", project_name);
    }
}

fn is_benign_error(e: &hyper::Error) -> bool {
    if e.is_incomplete_message() || e.is_canceled() {
        return true;
    }
    let msg = e.to_string();
    msg.contains("connection reset") || msg.contains("broken pipe")
}

// ---------------------------------------------------------------------------
// Request handling — forward HTTP, upgrade WebSockets
// ---------------------------------------------------------------------------

async fn handle_request(
    req: Request<Incoming>,
    routes: Arc<RwLock<HashMap<String, u16>>>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let host = req
        .headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_string();

    let port = {
        let routes = routes.read().unwrap();
        routes.get(&host).copied()
    };

    let port = match port {
        Some(p) => p,
        None => {
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                &format!("No upstream for host: {}", host),
            ));
        }
    };

    // Check for WebSocket upgrade
    if is_websocket_upgrade(&req) {
        return handle_websocket_upgrade(req, port).await;
    }

    // Forward as normal HTTP request
    forward_http(req, port).await
}

fn is_websocket_upgrade(req: &Request<Incoming>) -> bool {
    req.headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
}

async fn forward_http(
    req: Request<Incoming>,
    port: u16,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

    let upstream_url = format!("http://localhost:{}{}", port, path);

    // Build upstream request with reqwest
    let client = reqwest::Client::new();
    let mut builder = client.request(method.clone(), &upstream_url);

    // Copy non-hop-by-hop headers
    for (name, value) in req.headers() {
        if !is_hop_by_hop(name.as_str()) {
            builder = builder.header(name.clone(), value.clone());
        }
    }

    // Forward body
    let body_bytes = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            eprintln!("Proxy: failed to read request body: {}", e);
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                "Failed to read request body",
            ));
        }
    };

    if !body_bytes.is_empty() {
        builder = builder.body(body_bytes.to_vec());
    }

    let upstream_resp = match builder.send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Proxy: upstream error for {}: {}", upstream_url, e);
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                &format!("Upstream error: {}", e),
            ));
        }
    };

    // Build response
    let mut response = Response::builder().status(upstream_resp.status());

    for (name, value) in upstream_resp.headers() {
        if !is_hop_by_hop(name.as_str()) {
            response = response.header(name.clone(), value.clone());
        }
    }

    let body_bytes = upstream_resp.bytes().await.unwrap_or_default();
    let body = Full::new(body_bytes)
        .map_err(|never| match never {})
        .boxed();

    Ok(response.body(body).unwrap())
}

async fn handle_websocket_upgrade(
    req: Request<Incoming>,
    port: u16,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let uri = req.uri().clone();
    let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

    // Connect to upstream
    let upstream_addr = format!("127.0.0.1:{}", port);
    let mut upstream = match TcpStream::connect(&upstream_addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Proxy: WS upstream connect failed: {}", e);
            return Ok(error_response(
                StatusCode::BAD_GATEWAY,
                &format!("WebSocket upstream connect failed: {}", e),
            ));
        }
    };

    // Send the upgrade request to upstream as raw HTTP
    let mut upgrade_req = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n",
        path, port
    );

    for (name, value) in req.headers() {
        if !name.as_str().eq_ignore_ascii_case("host") {
            if let Ok(v) = value.to_str() {
                upgrade_req.push_str(&format!("{}: {}\r\n", name, v));
            }
        }
    }
    upgrade_req.push_str("\r\n");

    if let Err(e) = upstream.write_all(upgrade_req.as_bytes()).await {
        eprintln!("Proxy: WS upstream write failed: {}", e);
        return Ok(error_response(
            StatusCode::BAD_GATEWAY,
            "WebSocket upstream write failed",
        ));
    }

    // Read the upstream response (just the status line + headers)
    let mut resp_buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 1];
    loop {
        match upstream.read(&mut tmp).await {
            Ok(0) => break,
            Ok(_) => {
                resp_buf.push(tmp[0]);
                if resp_buf.len() >= 4
                    && &resp_buf[resp_buf.len() - 4..] == b"\r\n\r\n"
                {
                    break;
                }
                if resp_buf.len() > 8192 {
                    return Ok(error_response(
                        StatusCode::BAD_GATEWAY,
                        "WebSocket upstream response too large",
                    ));
                }
            }
            Err(e) => {
                eprintln!("Proxy: WS upstream read failed: {}", e);
                return Ok(error_response(
                    StatusCode::BAD_GATEWAY,
                    "WebSocket upstream read failed",
                ));
            }
        }
    }

    // Parse the upstream response to extract status and headers
    let resp_str = String::from_utf8_lossy(&resp_buf);
    let mut lines = resp_str.lines();

    let status_line = lines.next().unwrap_or("HTTP/1.1 502 Bad Gateway");
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(502);

    let mut response_builder =
        Response::builder().status(StatusCode::from_u16(status_code).unwrap_or(StatusCode::BAD_GATEWAY));

    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(": ") {
            response_builder = response_builder.header(name, value.trim());
        }
    }

    if status_code != 101 {
        let body = Full::new(Bytes::from("WebSocket upgrade rejected by upstream"))
            .map_err(|never| match never {})
            .boxed();
        return Ok(response_builder.body(body).unwrap());
    }

    // Perform the hyper upgrade on the client side, then bridge both streams
    tokio::spawn(async move {
        let upgraded = match hyper::upgrade::on(req).await {
            Ok(u) => u,
            Err(e) => {
                eprintln!("Proxy: client upgrade failed: {}", e);
                return;
            }
        };

        let mut client_io = hyper_util::rt::TokioIo::new(upgraded);

        let (mut upstream_read, mut upstream_write) = upstream.into_split();
        let (mut client_read, mut client_write) =
            tokio::io::split(&mut client_io);

        let c2u = tokio::io::copy(&mut client_read, &mut upstream_write);
        let u2c = tokio::io::copy(&mut upstream_read, &mut client_write);

        let _ = tokio::try_join!(c2u, u2c);
    });

    // Return the 101 Switching Protocols response to the client
    let body = Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed();
    Ok(response_builder.body(body).unwrap())
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn error_response(
    status: StatusCode,
    message: &str,
) -> Response<BoxBody<Bytes, hyper::Error>> {
    let body = Full::new(Bytes::from(message.to_string()))
        .map_err(|never| match never {})
        .boxed();
    Response::builder()
        .status(status)
        .body(body)
        .unwrap()
}
