use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::post,
};

use crate::{
    AppState,
    capture::IncomingRequest,
    providers::{ProviderError, ProviderOutcome, apply_faults_and_respond},
    render::{RenderedMessage, blockkit},
    store::ChannelKind,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/slack/services/{team}/{bot}/{token}", post(handle_slack))
}

async fn handle_slack(
    State(state): State<AppState>,
    Path((team, bot, token)): Path<(String, String, String)>,
    req: axum::extract::Request,
) -> Response {
    let incoming = match IncomingRequest::from_request(req).await {
        Ok(i) => i,
        Err(r) => return r,
    };

    let Some(channel) = state.store.get_or_create_by_path(
        ChannelKind::Slack,
        &token,
        Some(&team),
        Some(&bot),
        state.auto_create,
    ) else {
        return StatusCode::NOT_FOUND.into_response_plain("no_service");
    };

    let parsed = match incoming.parse_slack_body() {
        Ok(v) => v,
        Err(err) => {
            return apply_faults_and_respond(
                &state,
                &channel,
                incoming,
                ProviderOutcome {
                    warnings: vec![format!("invalid_payload: {err}")],
                    rendered: RenderedMessage::plain_text("(invalid payload)"),
                    ok_status: StatusCode::OK,
                    ok_headers: vec![("content-type".into(), "text/plain".into())],
                    ok_body: "ok".into(),
                    error: Some(ProviderError {
                        status: StatusCode::BAD_REQUEST,
                        headers: vec![("content-type".into(), "text/plain".into())],
                        body: "invalid_payload".into(),
                    }),
                },
            )
            .await;
        }
    };

    let parsed_bk = blockkit::parse_slack_payload(&parsed);
    let mut warnings = parsed_bk.warnings;
    let error = if warnings.iter().any(|w| w.starts_with("missing text")) {
        warnings.retain(|w| !w.starts_with("missing text"));
        Some(ProviderError {
            status: StatusCode::BAD_REQUEST,
            headers: vec![("content-type".into(), "text/plain".into())],
            body: "no_text".into(),
        })
    } else {
        None
    };

    apply_faults_and_respond(
        &state,
        &channel,
        incoming,
        ProviderOutcome {
            warnings,
            rendered: parsed_bk.message,
            ok_status: StatusCode::OK,
            ok_headers: vec![("content-type".into(), "text/plain".into())],
            ok_body: "ok".into(),
            error,
        },
    )
    .await
}

trait PlainResponse {
    fn into_response_plain(self, body: &'static str) -> Response;
}

impl PlainResponse for StatusCode {
    fn into_response_plain(self, body: &'static str) -> Response {
        use axum::response::IntoResponse;
        (self, body).into_response()
    }
}
