//! Lightweight HTTP surface for the coordinator.
//!
//! Serves Prometheus metrics, a liveness probe, and a readiness probe on a
//! separate port from the gRPC API (default `0.0.0.0:8081`). This makes
//! coordinator gauges scrapable and gives orchestrators a dependency-aware
//! readiness signal.

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::future::BoxFuture;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Asynchronous readiness check; returns `true` when dependencies are healthy.
pub type ReadyCheck = Arc<dyn Fn() -> BoxFuture<'static, bool> + Send + Sync>;

/// Build the coordinator HTTP router.
pub fn router(ready: ReadyCheck) -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .route("/health", get(health))
        .route("/ready", get(ready_handler))
        .with_state(ready)
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

/// Liveness probe (process is up).
pub async fn health() -> Response {
    json_response(StatusCode::OK, r#"{"status":"ok","service":"coordinator"}"#)
}

/// Readiness probe (dependencies are reachable).
pub async fn ready_handler(State(check): State<ReadyCheck>) -> Response {
    readiness_response(check().await)
}

fn readiness_response(ready: bool) -> Response {
    if ready {
        json_response(StatusCode::OK, r#"{"status":"ready"}"#)
    } else {
        json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            r#"{"status":"not_ready","reason":"database unavailable"}"#,
        )
    }
}

fn json_response(status: StatusCode, body: &'static str) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// Bind `addr` and serve the HTTP surface until the process exits.
pub async fn serve(addr: SocketAddr, ready: ReadyCheck) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve_listener(listener, ready).await
}

/// Serve the HTTP surface on an already-bound listener.
pub async fn serve_listener(listener: TcpListener, ready: ReadyCheck) -> std::io::Result<()> {
    let addr = listener.local_addr()?;
    tracing::info!("coordinator metrics/health listening on {addr}");
    axum::serve(listener, router(ready)).await
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

    fn stub_ready(result: bool) -> ReadyCheck {
        Arc::new(move || Box::pin(async move { result }))
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
    async fn ready_reports_200_when_dependencies_ok() {
        let resp = ready_handler(State(stub_ready(true))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_text(resp).await;
        assert!(body.contains("\"status\":\"ready\""), "body: {body}");
    }

    #[tokio::test]
    async fn ready_reports_503_when_dependencies_down() {
        let resp = ready_handler(State(stub_ready(false))).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = body_text(resp).await;
        assert!(body.contains("\"status\":\"not_ready\""), "body: {body}");
    }

    #[tokio::test]
    async fn server_serves_metrics_over_http() {
        observability::register_metrics().expect("register metrics");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            let _ = serve_listener(listener, stub_ready(true)).await;
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
