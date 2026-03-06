use axum::{
    extract::{Json, State as AxumState},
    http::StatusCode,
    routing::{delete, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

use crate::mdns::MdnsRegistrar;
use crate::proxy::{ProxyManager, TlsManager};

/// Shared state for the host API server.
pub struct HostApiState {
    pub proxy: Arc<ProxyManager>,
    pub tls_manager: Arc<TlsManager>,
    pub mdns: Arc<MdnsRegistrar>,
}

#[derive(Deserialize)]
struct OpenUrlRequest {
    url: String,
}

async fn open_url(Json(payload): Json<OpenUrlRequest>) -> Result<StatusCode, (StatusCode, String)> {
    let url = &payload.url;

    // Only allow https:// URLs (and http://localhost for dev)
    if !url.starts_with("https://") && !url.starts_with("http://localhost") {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("URL must start with https:// (got: {})", url),
        ));
    }

    // Open in default browser via macOS `open` command
    let result = std::process::Command::new("open").arg(url).status();

    match result {
        Ok(status) if status.success() => Ok(StatusCode::OK),
        Ok(status) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("open command exited with: {}", status),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to run open command: {}", e),
        )),
    }
}

#[derive(Deserialize)]
struct AddRouteRequest {
    project_name: String,
    service_name: String,
    port: u16,
}

#[derive(Serialize)]
struct AddRouteResponse {
    hostname: String,
    url: String,
}

async fn add_route(
    AxumState(state): AxumState<Arc<HostApiState>>,
    Json(payload): Json<AddRouteRequest>,
) -> Result<Json<AddRouteResponse>, (StatusCode, String)> {
    let project_name = &payload.project_name;
    let service_name = &payload.service_name;
    let port = payload.port;

    // Register mDNS hostname (idempotent — if already registered, skip)
    if !state.mdns.is_registered(project_name) {
        state.mdns.register(project_name).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("mDNS registration failed: {}", e),
            )
        })?;
    }

    // Add reverse proxy route
    let hostname = MdnsRegistrar::hostname_for_project(project_name);
    state.proxy.add_route(&hostname, port, &state.tls_manager);

    let url = format!("https://{}:4443", hostname);

    Ok(Json(AddRouteResponse { hostname, url }))
}

#[derive(Deserialize)]
struct RemoveRouteRequest {
    project_name: String,
    service_name: String,
}

async fn remove_route(
    AxumState(state): AxumState<Arc<HostApiState>>,
    Json(payload): Json<RemoveRouteRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Remove proxy route
    let hostname = MdnsRegistrar::hostname_for_project(&payload.project_name);
    state.proxy.remove_route(&hostname);

    // Note: we don't deregister mDNS here because other services may still use the hostname.
    // mDNS deregistration happens when all routes for a project are removed (delete_project).

    Ok(StatusCode::OK)
}

/// Start the host API server on the given port. Returns the actual bound port.
pub async fn start(port: u16, state: Arc<HostApiState>) -> Result<u16, String> {
    let app = Router::new()
        .route("/open-url", post(open_url))
        .route("/routes", post(add_route))
        .route("/routes", delete(remove_route))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Failed to bind host API on port {}: {}", port, e))?;

    let bound_port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local addr: {}", e))?
        .port();

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("Host API server error: {}", e);
        }
    });

    Ok(bound_port)
}
