//! RPC/dashboard HTTP surface (axum): health, Prometheus metrics, and a
//! bearer-protected `/action` stub.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use once_cell::sync::OnceCell;
use serde_json::{json, Value};
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;

use lumi_kernel::services::Services;
use lumi_kernel::Result;

static PROMETHEUS: OnceCell<PrometheusHandle> = OnceCell::new();

/// Install the global Prometheus recorder. Idempotent.
pub fn init_metrics() {
    if PROMETHEUS.get().is_some() {
        return;
    }
    match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => {
            let _ = PROMETHEUS.set(handle);
        }
        Err(err) => tracing::warn!(error = %err, "failed to install prometheus recorder"),
    }
}

fn render_metrics() -> String {
    PROMETHEUS
        .get()
        .map_or_else(String::new, PrometheusHandle::render)
}

#[derive(Clone)]
struct RpcState {
    token: Option<String>,
    services: Arc<Services>,
}

/// Serve the RPC surface on `addr`. `token` is the expected bearer token;
/// `None` leaves `/action` unauthenticated (loopback/dev only).
pub async fn serve(addr: SocketAddr, token: Option<String>, services: Arc<Services>) -> Result<()> {
    let state = RpcState { token, services };

    // Per-IP burst limit on the mutating endpoint; falls back to governor defaults.
    let governor_conf = GovernorConfigBuilder::default()
        .per_second(2)
        .burst_size(20)
        .finish()
        .unwrap_or_default();

    let app = Router::new()
        .route("/action", post(action))
        .route_layer(GovernorLayer::new(governor_conf))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics))
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

async fn metrics() -> impl IntoResponse {
    let body = tokio::task::spawn_blocking(render_metrics)
        .await
        .unwrap_or_default();
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

async fn action(State(state): State<RpcState>, body: Option<Json<Value>>) -> impl IntoResponse {
    tracing::debug!(role = ?state.services.cfg.role, "rpc /action stub");
    let _ = body;
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({ "error": "action dispatch not implemented" })),
    )
}

async fn auth(State(state): State<RpcState>, req: Request, next: Next) -> Response {
    if let Some(expected) = &state.token
        && !bearer_matches(req.headers(), expected)
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(req).await
}

fn bearer_matches(headers: &HeaderMap, expected: &str) -> bool {
    let Some(presented) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    constant_time_eq(presented.as_bytes(), expected.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn bearer_requires_exact_match() {
        let mut headers = HeaderMap::new();
        assert!(!bearer_matches(&headers, "secret"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic secret"),
        );
        assert!(!bearer_matches(&headers, "secret"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        assert!(bearer_matches(&headers, "secret"));
        assert!(!bearer_matches(&headers, "secrez"));
        assert!(!bearer_matches(&headers, "secret-longer"));
    }
}
