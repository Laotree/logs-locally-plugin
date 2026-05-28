/// Multi-user relay server.
///
/// Receives `llp push` requests from many users, derives an anonymous hash
/// from each user's token, then forwards the pre-rendered SVG to a Cloudflare
/// Worker (or any compatible backend) under `/chart/<hash>.svg`.
///
/// Configuration (env vars):
///   LLP_CF_WORKER_URL   — URL of the target CF Worker (required)
///   LLP_CF_PUSH_TOKEN   — Bearer token for the CF Worker's /api/push (required)
///
/// Listen port: set with `--port` flag (default: 8485)
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Clone)]
pub struct RelayState {
    pub cf_worker_url: String,
    pub cf_push_token: String,
    pub http: reqwest::Client,
}

pub fn router(state: RelayState) -> Router {
    Router::new()
        .route("/api/push", post(handle_push))
        .with_state(Arc::new(state))
}

/// Derive a stable, anonymous 16-char hex key from a token.
/// Same token always produces the same key; the key cannot be reversed to
/// recover the token.
pub fn token_hash(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    format!("{:.16}", hex::encode(h.finalize()))
}

async fn handle_push(
    State(state): State<Arc<RelayState>>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Extract bearer token — it is the sole user identity
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if token.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "missing Authorization: Bearer <token>"})),
        )
            .into_response();
    }

    let svg = match payload["svg"].as_str() {
        Some(s) => s.to_string(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing svg field"})),
            )
                .into_response()
        }
    };

    let hash = token_hash(token);

    // Forward to CF Worker with the anonymous hash as the routing key
    let target = format!("{}/api/push", state.cf_worker_url.trim_end_matches('/'));
    match state
        .http
        .post(&target)
        .bearer_auth(&state.cf_push_token)
        .json(&serde_json::json!({"hash": hash, "svg": svg}))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            let chart_url = format!(
                "{}/chart/{}.svg",
                state.cf_worker_url.trim_end_matches('/'),
                hash
            );
            Json(serde_json::json!({"ok": true, "chart_url": chart_url})).into_response()
        }
        Ok(r) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("CF push failed: {}", r.status())})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
