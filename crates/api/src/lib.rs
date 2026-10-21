//! RPC/dashboard HTTP surface (axum): health, Prometheus metrics, and typed action dispatch.

use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;

use lumi_kernel::services::Services;
use lumi_kernel::{Config, Result};

mod interactions;

static PROMETHEUS: LazyLock<Option<PrometheusHandle>> =
    LazyLock::new(|| match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => Some(handle),
        Err(err) => {
            tracing::warn!(error = %err, "failed to install prometheus recorder");
            None
        }
    });

/// Install the global Prometheus recorder. Idempotent.
pub fn init_metrics() {
    LazyLock::force(&PROMETHEUS);
}

fn render_metrics() -> String {
    PROMETHEUS
        .as_ref()
        .map_or_else(String::new, PrometheusHandle::render)
}

#[derive(Clone)]
struct RpcState {
    token: Option<String>,
    services: Arc<Services>,
}

#[derive(Deserialize, Debug)]
pub struct ActionRequest {
    pub action: String,
    pub payload: Option<Value>,
}

#[derive(Serialize, Debug)]
pub struct ActionResponse {
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
}

/// Serve the RPC surface on `addr`. `token` is the expected bearer token;
/// `None` leaves `/action` unauthenticated (loopback/dev only).
pub async fn serve(
    addr: SocketAddr,
    token: Option<String>,
    config: &Config,
    services: Arc<Services>,
) -> Result<()> {
    let state = RpcState { token, services };

    let governor_conf = GovernorConfigBuilder::default()
        .per_second(5)
        .burst_size(20)
        .finish()
        .unwrap_or_default();

    let interactions_router = interactions::router(config);

    let api_routes = Router::new()
        .route("/action", post(action))
        .route_layer(GovernorLayer::new(governor_conf))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics))
        .with_state(state);

    let app = api_routes
        .merge(interactions_router)
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

/// Action dispatcher: handles operational RPC requests from dashboards or automation.
async fn action(
    State(state): State<RpcState>,
    Json(req): Json<ActionRequest>,
) -> impl IntoResponse {
    tracing::info!(action = %req.action, "executing rpc action");

    match req.action.as_str() {
        "ping" => (
            StatusCode::OK,
            Json(ActionResponse {
                success: true,
                data: Some(json!({ "status": "pong", "time": chrono::Utc::now().to_rfc3339() })),
                error: None,
            }),
        ),
        "cache.clear" => (
            StatusCode::OK,
            Json(ActionResponse {
                success: true,
                data: Some(json!({ "message": "cache invalidation triggered" })),
                error: None,
            }),
        ),
        "modules.list" => {
            let modules: Vec<_> = state
                .services
                .registry
                .iter()
                .map(|m| {
                    let meta = m.metadata();
                    json!({
                        "name": m.name(),
                        "display_name": meta.display_name,
                        "emoji": meta.emoji,
                        "version": meta.version,
                    })
                })
                .collect();

            (
                StatusCode::OK,
                Json(ActionResponse {
                    success: true,
                    data: Some(json!({ "modules": modules })),
                    error: None,
                }),
            )
        }
        "security.panic.status" => {
            if let Some(guild_id) = req
                .payload
                .as_ref()
                .and_then(|payload| payload.get("guild_id"))
                .and_then(Value::as_str)
            {
                match state.services.security.get_panic_status(guild_id).await {
                    Ok(st) => {
                        return (
                            StatusCode::OK,
                            Json(ActionResponse {
                                success: true,
                                data: Some(json!({ "panic": st })),
                                error: None,
                            }),
                        );
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ActionResponse {
                                success: false,
                                data: None,
                                error: Some(e.to_string()),
                            }),
                        );
                    }
                }
            }
            (
                StatusCode::BAD_REQUEST,
                Json(ActionResponse {
                    success: false,
                    data: None,
                    error: Some("missing guild_id in payload".to_string()),
                }),
            )
        }
        _ => (
            StatusCode::NOT_FOUND,
            Json(ActionResponse {
                success: false,
                data: None,
                error: Some(format!("unknown action: {}", req.action)),
            }),
        ),
    }
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
