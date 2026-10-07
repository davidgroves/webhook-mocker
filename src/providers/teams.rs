use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};

use crate::{
    AppState,
    capture::IncomingRequest,
    providers::{ProviderOutcome, apply_faults_and_respond},
    render::{RenderedMessage, adaptive, messagecard},
    store::ChannelKind,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/teams/workflows/{id}", post(handle_workflows))
        .route("/teams/webhookb2/{id}", post(handle_legacy))
}

async fn handle_workflows(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> Response {
    let incoming = match IncomingRequest::from_request(req).await {
        Ok(i) => i,
        Err(r) => return r,
    };

    let Some(channel) = state.store.get_or_create_by_path(
        ChannelKind::TeamsWorkflows,
        &id,
        None,
        None,
        state.auto_create,
    ) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    let (warnings, rendered) = match incoming.body_json() {
        Some(v) => {
            let p = adaptive::parse_teams_workflows_payload(&v);
            (p.warnings, p.message)
        }
        None => (
            vec!["invalid JSON body".into()],
            RenderedMessage::plain_text(
                incoming
                    .body_utf8()
                    .unwrap_or_else(|| format!("<{} bytes>", incoming.body.len())),
            ),
        ),
    };

    apply_faults_and_respond(
        &state,
        &channel,
        incoming,
        ProviderOutcome {
            warnings,
            rendered,
            ok_status: StatusCode::ACCEPTED,
            ok_headers: vec![("content-type".into(), "text/plain".into())],
            ok_body: String::new(),
            error: None,
        },
    )
    .await
}

async fn handle_legacy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> Response {
    let incoming = match IncomingRequest::from_request(req).await {
        Ok(i) => i,
        Err(r) => return r,
    };

    let Some(channel) = state.store.get_or_create_by_path(
        ChannelKind::TeamsLegacy,
        &id,
        None,
        None,
        state.auto_create,
    ) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    let (warnings, rendered) = match incoming.body_json() {
        Some(v) => {
            let p = messagecard::parse_legacy_teams_payload(&v);
            (p.warnings, p.message)
        }
        None => (
            vec!["invalid JSON body".into()],
            RenderedMessage::plain_text(
                incoming
                    .body_utf8()
                    .unwrap_or_else(|| format!("<{} bytes>", incoming.body.len())),
            ),
        ),
    };

    apply_faults_and_respond(
        &state,
        &channel,
        incoming,
        ProviderOutcome {
            warnings,
            rendered,
            ok_status: StatusCode::OK,
            ok_headers: vec![("content-type".into(), "text/plain".into())],
            ok_body: "1".into(),
            error: None,
        },
    )
    .await
}
