use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::any,
};

use crate::{
    AppState,
    capture::IncomingRequest,
    providers::{ProviderOutcome, apply_faults_and_respond},
    render::RenderedMessage,
    store::ChannelKind,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/hooks/{id}", any(handle_generic))
}

async fn handle_generic(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> Response {
    let incoming = match IncomingRequest::from_request(req).await {
        Ok(i) => i,
        Err(r) => return r,
    };

    let Some(channel) =
        state
            .store
            .get_or_create_by_path(ChannelKind::Generic, &id, None, None, state.auto_create)
    else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    let rendered = if let Some(v) = incoming.body_json() {
        if let Some(text) = v.get("text").and_then(|x| x.as_str()) {
            RenderedMessage::plain_text(text)
        } else {
            RenderedMessage::from_markdown(&format!("```json\n{}\n```", pretty(&v)))
        }
    } else if let Some(utf8) = incoming.body_utf8() {
        RenderedMessage::plain_text(utf8)
    } else {
        RenderedMessage::plain_text(format!("<{} binary bytes>", incoming.body.len()))
    };

    apply_faults_and_respond(
        &state,
        &channel,
        incoming,
        ProviderOutcome {
            warnings: Vec::new(),
            rendered,
            ok_status: StatusCode::OK,
            ok_headers: vec![("content-type".into(), "application/json".into())],
            ok_body: r#"{"ok":true}"#.into(),
            error: None,
        },
    )
    .await
}

fn pretty(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}
