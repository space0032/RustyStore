//! Lightweight HTTP surface for the coordinator.
//!
//! Serves Prometheus metrics and a health probe on a separate port from the
//! gRPC API (default `0.0.0.0:8081`). This makes coordinator gauges such as
//! `rustystore_active_storage_nodes` and `rustystore_under_replicated_chunks`
//! scrapable without exposing the gRPC endpoint.

use axum::{
    Router,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Build the coordinator HTTP router.
pub fn router() -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .route("/health", get(health))
}

/// Prometheus text exposition of all registered collectors.
pub async fn metrics() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/plain; version=0.0.4; charset=utf-8"
            .parse()
            .expect("valid content type"),
    );
    (headers, observability::prometheus_metrics()).into_response()
}

/// Liveness probe.
pub async fn health() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"ok","service":"coordinator"}"#,
    )
        .into_response()
}

/// Bind `addr` and serve the HTTP surface until the process exits.
pub async fn serve(addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve_listener(listener).await
}

/// Serve the HTTP surface on an already-bound listener.
pub async fn serve_listener(listener: TcpListener) -> std::io::Result<()> {
    let addr = listener.local_addr()?;
    tracing::info!("coordinator metrics/health listening on {addr}");
    axum::serve(listener, router()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_text(resp: Response) -> String {
        let bytes = to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        String::from_utf8(bytes.to_vec()).expect("utf-8 body")
    }

    #[tokio::test]
    async fn metrics_render_prometheus_text() {
        observability::register_metrics().expect("register metrics");
        let resp = metrics().await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_text(resp).await;
        assert!(body.contains("rustystore_active_storage_nodes"));
        assert!(body.contains("rustystore_under_replicated_chunks"));
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let resp = health().await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_text(resp).await;
        assert!(body.contains("\"service\":\"coordinator\""));
    }

    #[tokio::test]
    async fn server_serves_metrics_over_http() {
        observability::register_metrics().expect("register metrics");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            let _ = serve_listener(listener).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let response = tokio::task::spawn_blocking(move || {
            use std::io::{Read, Write};
            let mut stream = std::net::TcpStream::connect(addr).expect("connect");
            stream
                .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .expect("write request");
            let mut buf = String::new();
            stream.read_to_string(&mut buf).expect("read response");
            buf
        })
        .await
        .expect("join client");

        assert!(
            response.contains("200 OK"),
            "unexpected response: {response}"
        );
        assert!(response.contains("rustystore_"));
    }
}
