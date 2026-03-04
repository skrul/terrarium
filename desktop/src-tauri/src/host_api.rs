use axum::{extract::Json, http::StatusCode, routing::post, Router};
use serde::Deserialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;

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

/// Start the host API server on the given port. Returns the actual bound port.
pub async fn start(port: u16) -> Result<u16, String> {
    let app = Router::new().route("/open-url", post(open_url));

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
