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

    match state.db.list_sessions(
        query.model.as_deref(),
        query.since.as_deref(),
        query.q.as_deref(),
        limit,
        offset,
    ) {
        Ok(sessions) => Json(serde_json::json!({ "sessions": sessions })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_session(&id) {
        Ok(Some(session)) => Json(serde_json::json!(session)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "session not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
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
