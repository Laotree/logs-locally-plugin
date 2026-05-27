use crate::chart::{ActivityData, DayRecord};
use crate::db::Db;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    /// Aggregated activity data received via POST /api/push.
    pub activity: Arc<RwLock<ActivityData>>,
    /// Token required in `Authorization: Bearer <token>` for /api/push.
    pub push_token: String,
    /// Optional path to persist activity.json across restarts.
    pub data_path: Option<std::path::PathBuf>,
}

#[derive(Deserialize)]
pub struct SessionQuery {
    model: Option<String>,
    source: Option<String>,
    since: Option<String>,
    q: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub struct RouterConfig {
    pub push_token: String,
    pub data_path: Option<std::path::PathBuf>,
}

pub fn router(db: Db, cfg: RouterConfig) -> Router {
    // Load persisted activity data from disk if available
    let initial_activity = cfg.data_path.as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<ActivityData>(&s).ok())
        .unwrap_or_default();

    let state = AppState {
        db: Arc::new(db),
        activity: Arc::new(RwLock::new(initial_activity)),
        push_token: cfg.push_token,
        data_path: cfg.data_path,
    };

    Router::new()
        .route("/", get(index_html))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id", get(get_session))
        .route("/api/sessions/:id/messages", get(get_messages))
        .route("/api/sessions/:id/score", get(get_score))
        .route("/api/stats", get(get_stats))
        .route("/api/score-stats", get(get_score_stats))
        .route("/api/activity", get(get_activity))
        .route("/api/push", post(handle_push))
        .route("/chart.svg", get(get_chart_svg))
        .with_state(state)
}

async fn index_html() -> Html<&'static str> {
    Html(include_str!("web/index.html"))
}

async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<SessionQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    let sessions = match state.db.list_sessions(
        query.model.as_deref(),
        query.source.as_deref(),
        query.since.as_deref(),
        query.q.as_deref(),
        limit,
        offset,
    ) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
    let scores = state.db.get_scores_for_sessions(&ids).unwrap_or_default();

    let sessions_json: Vec<serde_json::Value> = sessions
        .into_iter()
        .map(|s| {
            let id = s.id.clone();
            let mut v = serde_json::to_value(&s).unwrap_or(serde_json::Value::Null);
            if let serde_json::Value::Object(ref mut map) = v {
                let score_val = scores.get(&id).map(|sc| {
                    serde_json::json!({
                        "total": sc.total_score,
                        "grade": sc.grade,
                    })
                });
                map.insert("score".into(), score_val.unwrap_or(serde_json::Value::Null));
            }
            v
        })
        .collect();

    Json(serde_json::json!({ "sessions": sessions_json })).into_response()
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let session = match state.db.get_session(&id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "session not found" })),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let score = state.db.get_score(&id).ok().flatten();
    let mut v = serde_json::to_value(&session).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(ref mut map) = v {
        map.insert(
            "score".into(),
            score
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    Json(v).into_response()
}

async fn get_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_messages(&id) {
        Ok(messages) => Json(serde_json::json!({ "messages": messages })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn get_score(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_score(&id) {
        Ok(Some(score)) => Json(serde_json::to_value(score).unwrap_or(serde_json::Value::Null))
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "score not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SinceQuery {
    since: Option<String>,
}

async fn get_stats(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    match state.db.get_stats(q.since.as_deref()) {
        Ok(stats) => Json(serde_json::json!(stats)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn get_score_stats(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    match state.db.get_score_aggregates(q.since.as_deref()) {
        Ok(data) => Json(data).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn get_activity(
    State(state): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    match state.db.get_daily_activity(q.since.as_deref()) {
        Ok(data) => Json(serde_json::json!({ "days": data })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Receive aggregated daily stats from `llp push`.
/// Only accepts: day, session_count, token_count — no raw session content.
async fn handle_push(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Token validation (skip if push_token is empty)
    if !state.push_token.is_empty() {
        let provided = headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or("");
        if provided != state.push_token {
            return (StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": "unauthorized"}))).into_response();
        }
    }

    let days: Vec<DayRecord> = payload["days"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();

    let count = days.len();
    let updated_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let new_data = ActivityData { days, updated_at };

    // Persist to disk if data_path is configured
    if let Some(ref path) = state.data_path {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, serde_json::to_string(&new_data).unwrap_or_default());
    }

    *state.activity.write().unwrap() = new_data;

    Json(serde_json::json!({"ok": true, "days": count})).into_response()
}

/// Return a dark SVG with two contribution-style heatmaps (sessions + tokens).
/// Safe to embed in a public GitHub profile README.
async fn get_chart_svg(State(state): State<AppState>) -> impl IntoResponse {
    let data = state.activity.read().unwrap().clone();
    let svg = crate::chart::render_svg(&data);
    (
        [(axum::http::header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"),
         (axum::http::header::CACHE_CONTROL, "no-cache, max-age=0")],
        svg,
    )
        .into_response()
}
