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
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct RelayState {
    pub cf_worker_url: String,
    pub cf_push_token: String,
    pub http: reqwest::Client,
    /// Max pushes per user per hour (sliding window).
    pub max_pushes_per_hour: usize,
    /// Per-user push timestamps for rate limiting.
    pub rate_limiter: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
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
    // Bearer token is optional. If present, use it as the user identity.
    // Otherwise, fall back to the `user` field from the push payload.
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

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

    // If no bearer token, derive identity from the `user` field sent by the client.
    let identity = if token.is_empty() {
        payload["user"].as_str().unwrap_or("anonymous")
    } else {
        token
    };
    let hash = token_hash(identity);

    // Rate limit: sliding 1-hour window per user hash
    {
        let now = Instant::now();
        let window = Duration::from_secs(3600);
        let mut limiter = state.rate_limiter.lock().unwrap();
        let queue = limiter.entry(hash.clone()).or_default();
        while queue.front().map_or(false, |t| now.duration_since(*t) > window) {
            queue.pop_front();
        }
        if queue.len() >= state.max_pushes_per_hour {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({"error": "rate limit exceeded"})),
            )
                .into_response();
        }
        queue.push_back(now);
    }

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
