use crate::db::Db;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        Html, IntoResponse, Json,
    },
    routing::get,
    Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub tx: broadcast::Sender<String>,
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
    let (tx, _) = broadcast::channel(100);
    let state = AppState {
        db: Arc::new(db),
        tx,
    };

    Router::new()
        .route("/", get(index_html))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id", get(get_session))
        .route("/api/sessions/:id/messages", get(get_messages))
        .route("/api/stats", get(get_stats))
        .route("/api/events", get(sse_handler))
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

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    Sse::new(EventStream { rx })
}

pub struct EventStream {
    rx: broadcast::Receiver<String>,
}

impl Stream for EventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.try_recv() {
            Ok(msg) => Poll::Ready(Some(Ok(Event::default().data(msg)))),
            Err(broadcast::error::TryRecvError::Closed) => Poll::Ready(None),
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                Poll::Ready(Some(Ok(Event::default().data("reconnect"))))
            }
            Err(broadcast::error::TryRecvError::Empty) => {
                // Short timeout poll to keep SSE alive
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

