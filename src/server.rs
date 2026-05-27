use crate::db::Db;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
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

pub fn router(db: Db) -> Router {
    let state = AppState {
        db: Arc::new(db),
    };

    Router::new()
        .route("/", get(index_html))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id", get(get_session))
        .route("/api/sessions/:id/messages", get(get_messages))
        .route("/api/sessions/:id/score", get(get_score))
        .route("/api/stats", get(get_stats))
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

async fn get_stats(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.get_stats() {
        Ok(stats) => Json(serde_json::json!(stats)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
