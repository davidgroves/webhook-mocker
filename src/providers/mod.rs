pub mod generic;
pub mod slack;
pub mod teams;

use axum::{Router, http::StatusCode, response::Response};

use crate::{
    AppState,
    capture::{IncomingRequest, response_meta},
    render::RenderedMessage,
    store::Channel,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(slack::router())
        .merge(teams::router())
        .merge(generic::router())
}

type HeaderPairs = Vec<(String, String)>;

pub struct ProviderOutcome {
    pub warnings: Vec<String>,
    pub rendered: RenderedMessage,
    pub ok_status: StatusCode,
    pub ok_headers: HeaderPairs,
    pub ok_body: String,
    pub error: Option<ProviderError>,
}

pub struct ProviderError {
    pub status: StatusCode,
    pub headers: HeaderPairs,
    pub body: String,
}

pub async fn apply_faults_and_respond(
    state: &AppState,
    channel: &Channel,
    incoming: IncomingRequest,
    outcome: ProviderOutcome,
) -> Response {
    let ProviderOutcome {
        mut warnings,
        rendered,
        ok_status,
        ok_headers,
        ok_body,
        error,
    } = outcome;

    let decision = channel.faults.decide();
    if !decision.delay.is_zero() {
        tokio::time::sleep(decision.delay).await;
    }

    if state.strict && !warnings.is_empty() && error.is_none() {
        let msg = warnings.join("; ");
        let (meta, resp) = response_meta(
            StatusCode::BAD_REQUEST,
            vec![("content-type".into(), "text/plain".into())],
            msg,
            incoming.started,
        );
        let capture = incoming.into_capture(channel, warnings, rendered, meta);
        state.store.push_capture(capture);
        return resp;
    }

    if let Some(err) = error {
        let (meta, resp) = response_meta(err.status, err.headers, err.body, incoming.started);
        let capture = incoming.into_capture(channel, warnings, rendered, meta);
        state.store.push_capture(capture);
        return resp;
    }

    if let Some(status) = decision.override_status {
        let mut headers = ok_headers.clone();
        if let Some(ra) = decision.retry_after_secs {
            headers.push(("retry-after".into(), ra.to_string()));
        }
        let body = if status == 429 {
            "rate_limited"
        } else {
            "fault_injected"
        };
        let (meta, resp) = response_meta(
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            headers,
            body,
            incoming.started,
        );
        let capture = incoming.into_capture(channel, warnings, rendered, meta);
        state.store.push_capture(capture);
        return resp;
    }

    if channel.archived {
        warnings.push("channel is archived".into());
        let (meta, resp) = response_meta(
            StatusCode::GONE,
            vec![("content-type".into(), "text/plain".into())],
            "channel_is_archived",
            incoming.started,
        );
        let capture = incoming.into_capture(channel, warnings, rendered, meta);
        state.store.push_capture(capture);
        return resp;
    }

    let (meta, resp) = response_meta(ok_status, ok_headers, ok_body, incoming.started);
    let capture = incoming.into_capture(channel, warnings, rendered, meta);
    state.store.push_capture(capture);
    resp
}
