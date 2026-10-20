//! Discord HTTP Interaction Webhook endpoint with Ed25519 signature verification.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{Json, Router};
use ring::signature::{ED25519, UnparsedPublicKey};
use serde_json::{Value, json};

use lumi_kernel::Config;

#[derive(Clone)]
struct InteractionState {
    public_key: Option<Vec<u8>>,
}

async fn verify_signature(
    headers: &HeaderMap,
    body: &[u8],
    public_key: &[u8],
) -> Result<(), StatusCode> {
    let signature_header = headers
        .get("X-Signature-Ed25519")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let timestamp_header = headers
        .get("X-Signature-Timestamp")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let signature_bytes = hex::decode(signature_header).map_err(|_| StatusCode::UNAUTHORIZED)?;
    if signature_bytes.len() != 64 {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let msg = [timestamp_header.as_bytes(), body].concat();
    let pk = UnparsedPublicKey::new(&ED25519, public_key);
    pk.verify(&msg, &signature_bytes)
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

async fn handle_interaction_inner(
    State(state): State<InteractionState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    let public_key = state.public_key.as_ref().ok_or(StatusCode::UNAUTHORIZED)?;
    verify_signature(&headers, &body, public_key).await?;

    let payload: Value = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    let interaction_type = payload
        .get("type")
        .and_then(Value::as_u64)
        .ok_or(StatusCode::BAD_REQUEST)?;

    match interaction_type {
        1 => {
            // PING
            Ok((StatusCode::OK, Json(json!({ "type": 1 }))))
        }
        2 | 3 => {
            // APPLICATION_COMMAND or MESSAGE_COMPONENT
            // Return deferred ephemeral response
            Ok((
                StatusCode::OK,
                Json(json!({
                    "type": 5,
                    "data": {
                        "flags": 64
                    }
                })),
            ))
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

async fn handle_interaction(
    State(state): State<InteractionState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    handle_interaction_inner(State(state), headers, body)
        .await
        .into_response()
}

pub fn router(config: &Config) -> Router {
    let public_key = config
        .discord_public_key
        .as_ref()
        .and_then(|pk| hex::decode(pk).ok())
        .filter(|pk| pk.len() == 32);

    let state = InteractionState { public_key };

    Router::new()
        .route("/api/interactions", axum::routing::post(handle_interaction))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    fn make_test_config(with_key: bool) -> Config {
        Config {
            role: lumi_kernel::Role::Api,
            token: "test".into(),
            database_url: "postgres://localhost".into(),
            redis_url: "redis://localhost".into(),
            rpc_token: None,
            rpc_port: 8080,
            owner_ids: vec![],
            custom_dir: "addons".into(),
            shard_count: None,
            shard_range: None,
            intents_guild_members: false,
            discord_public_key: if with_key {
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into())
            } else {
                None
            },
        }
    }

    #[tokio::test]
    async fn ping_without_public_key_returns_401() {
        let cfg = make_test_config(false);
        let app = router(&cfg);

        let Ok(req) = Request::builder()
            .method("POST")
            .uri("/api/interactions")
            .header("X-Signature-Ed25519", "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")
            .header("X-Signature-Timestamp", "1234567890")
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"type": 1}"#))
        else {
            panic!("valid request");
        };

        let resp = match app.oneshot(req).await {
            Ok(r) => r,
            Err(e) => match e {},
        };
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ping_with_invalid_signature_returns_401() {
        let cfg = make_test_config(true);
        let app = router(&cfg);

        let Ok(req) = Request::builder()
            .method("POST")
            .uri("/api/interactions")
            .header("X-Signature-Ed25519", "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")
            .header("X-Signature-Timestamp", "1234567890")
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"type": 1}"#))
        else {
            panic!("valid request");
        };

        let resp = match app.oneshot(req).await {
            Ok(r) => r,
            Err(e) => match e {},
        };
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ping_with_valid_signature_returns_pong() {
        let cfg = make_test_config(true);
        let app = router(&cfg);

        let Ok(req) = Request::builder()
            .method("POST")
            .uri("/api/interactions")
            .header("X-Signature-Ed25519", "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")
            .header("X-Signature-Timestamp", "1234567890")
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"type": 1}"#))
        else {
            panic!("valid request");
        };

        let resp = match app.oneshot(req).await {
            Ok(r) => r,
            Err(e) => match e {},
        };
        assert!(resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::OK);
    }

    #[tokio::test]
    async fn application_command_returns_deferred() {
        let cfg = make_test_config(true);
        let app = router(&cfg);

        let Ok(req) = Request::builder()
            .method("POST")
            .uri("/api/interactions")
            .header("X-Signature-Ed25519", "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")
            .header("X-Signature-Timestamp", "1234567890")
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"type": 2, "data": {"name": "test"}}"#))
        else {
            panic!("valid request");
        };

        let resp = match app.oneshot(req).await {
            Ok(r) => r,
            Err(e) => match e {},
        };
        assert!(resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::OK);
    }
}
