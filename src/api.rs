use std::{convert::Infallible, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        IntoResponse, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use tokio_stream::StreamExt as _;
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

use crate::{
    AppState,
    faults::FaultProfile,
    store::{Capture, Channel, ChannelKind, StoreEvent},
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "webhook-mocker",
        description = "HTTP API for creating virtual Slack/Teams webhook channels and asserting captured messages in automated tests.",
        version = "0.1.0"
    ),
    paths(
        healthz,
        list_channels,
        create_channel,
        get_channel,
        delete_channel,
        list_messages,
        get_message,
        clear_messages,
        wait_messages,
        set_faults,
        set_archive,
        events,
    ),
    components(schemas(
        ChannelOut,
        CreateChannel,
        ArchiveBody,
        MessagesOut,
        MessageDetailOut,
        Channel,
        ChannelKind,
        Capture,
        FaultProfile,
        crate::store::CaptureResponse,
        crate::render::RenderedMessage,
    )),
    tags(
        (name = "health", description = "Liveness"),
        (name = "channels", description = "Virtual webhook channels"),
        (name = "messages", description = "Captured webhook deliveries"),
        (name = "events", description = "Live updates"),
    )
)]
pub struct ApiDoc;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/channels", get(list_channels).post(create_channel))
        .route(
            "/api/channels/{id}",
            get(get_channel).delete(delete_channel),
        )
        .route(
            "/api/channels/{id}/messages",
            get(list_messages).delete(clear_messages),
        )
        .route("/api/channels/{id}/messages/{mid}", get(get_message))
        .route("/api/channels/{id}/wait", get(wait_messages))
        .route("/api/channels/{id}/faults", post(set_faults))
        .route("/api/channels/{id}/archive", post(set_archive))
        .route("/api/events", get(events))
}

pub fn docs_router() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())
}

#[derive(Serialize, ToSchema)]
struct ChannelOut {
    #[serde(flatten)]
    #[schema(inline)]
    channel: Channel,
    webhook_url: String,
    webhook_path: String,
    message_count: usize,
}

#[derive(Serialize, ToSchema)]
struct MessagesOut {
    messages: Vec<Capture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timed_out: Option<bool>,
}

#[derive(Serialize, ToSchema)]
struct MessageDetailOut {
    message: Capture,
    curl: String,
}

fn channel_out(state: &AppState, ch: Channel) -> ChannelOut {
    let webhook_path = ch.webhook_path();
    let webhook_url = ch.webhook_url(&state.public_base);
    let message_count = state.store.message_count(ch.id);
    ChannelOut {
        channel: ch,
        webhook_url,
        webhook_path,
        message_count,
    }
}

#[utoipa::path(
    get,
    path = "/api/channels",
    tag = "channels",
    responses(
        (status = 200, description = "All channels", body = Vec<ChannelOut>)
    )
)]
async fn list_channels(State(state): State<AppState>) -> Json<Vec<ChannelOut>> {
    Json(
        state
            .store
            .list_channels()
            .into_iter()
            .map(|c| channel_out(&state, c))
            .collect(),
    )
}

#[derive(Deserialize, ToSchema)]
struct CreateChannel {
    kind: ChannelKind,
    name: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    slack_team: Option<String>,
    #[serde(default)]
    slack_bot: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/channels",
    tag = "channels",
    request_body = CreateChannel,
    responses(
        (status = 201, description = "Channel created", body = ChannelOut)
    )
)]
async fn create_channel(
    State(state): State<AppState>,
    Json(body): Json<CreateChannel>,
) -> (StatusCode, Json<ChannelOut>) {
    let ch = state.store.create_channel(
        body.kind,
        body.name,
        body.token,
        body.slack_team,
        body.slack_bot,
        FaultProfile::default(),
    );
    (StatusCode::CREATED, Json(channel_out(&state, ch)))
}

#[utoipa::path(
    get,
    path = "/api/channels/{id}",
    tag = "channels",
    params(("id" = Uuid, Path, description = "Channel id")),
    responses(
        (status = 200, description = "Channel", body = ChannelOut),
        (status = 404, description = "Not found")
    )
)]
async fn get_channel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ChannelOut>, StatusCode> {
    state
        .store
        .get_channel(id)
        .map(|c| Json(channel_out(&state, c)))
        .ok_or(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    delete,
    path = "/api/channels/{id}",
    tag = "channels",
    params(("id" = Uuid, Path, description = "Channel id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found")
    )
)]
async fn delete_channel(State(state): State<AppState>, Path(id): Path<Uuid>) -> StatusCode {
    if state.store.delete_channel(id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

#[utoipa::path(
    get,
    path = "/api/channels/{id}/messages",
    tag = "messages",
    params(("id" = Uuid, Path, description = "Channel id")),
    responses(
        (status = 200, description = "Captured messages", body = MessagesOut),
        (status = 404, description = "Channel not found")
    )
)]
async fn list_messages(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MessagesOut>, StatusCode> {
    if state.store.get_channel(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    state.store.mark_read(id);
    Ok(Json(MessagesOut {
        messages: state.store.list_messages(id),
        timed_out: None,
    }))
}

#[utoipa::path(
    get,
    path = "/api/channels/{id}/messages/{mid}",
    tag = "messages",
    params(
        ("id" = Uuid, Path, description = "Channel id"),
        ("mid" = Uuid, Path, description = "Message id"),
    ),
    responses(
        (status = 200, description = "Message with replay curl", body = MessageDetailOut),
        (status = 404, description = "Not found")
    )
)]
async fn get_message(
    State(state): State<AppState>,
    Path((id, mid)): Path<(Uuid, Uuid)>,
) -> Result<Json<MessageDetailOut>, StatusCode> {
    state
        .store
        .get_message(id, mid)
        .map(|m| {
            Json(MessageDetailOut {
                curl: m.curl(&state.public_base),
                message: m,
            })
        })
        .ok_or(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    delete,
    path = "/api/channels/{id}/messages",
    tag = "messages",
    params(("id" = Uuid, Path, description = "Channel id")),
    responses(
        (status = 204, description = "Cleared"),
        (status = 404, description = "Channel not found")
    )
)]
async fn clear_messages(State(state): State<AppState>, Path(id): Path<Uuid>) -> StatusCode {
    if state.store.clear_messages(id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

#[derive(Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
struct WaitQuery {
    /// Stop when at least this many messages exist (default 1).
    #[serde(default = "default_count")]
    #[param(default = 1)]
    count: usize,
    /// How long to wait, e.g. `5s` or `500ms` (default `5s`).
    #[serde(default = "default_timeout")]
    #[param(default = "5s")]
    timeout: String,
}

fn default_count() -> usize {
    1
}
fn default_timeout() -> String {
    "5s".into()
}

fn parse_duration(s: &str) -> Duration {
    if let Some(ms) = s.strip_suffix("ms") {
        return Duration::from_millis(ms.parse().unwrap_or(5000));
    }
    if let Some(secs) = s.strip_suffix('s') {
        return Duration::from_secs(secs.parse().unwrap_or(5));
    }
    Duration::from_secs(s.parse().unwrap_or(5))
}

#[utoipa::path(
    get,
    path = "/api/channels/{id}/wait",
    tag = "messages",
    params(
        ("id" = Uuid, Path, description = "Channel id"),
        WaitQuery,
    ),
    responses(
        (status = 200, description = "Messages once count is reached (or timeout)", body = MessagesOut),
        (status = 404, description = "Channel not found")
    )
)]
async fn wait_messages(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<WaitQuery>,
) -> Result<Json<MessagesOut>, StatusCode> {
    if state.store.get_channel(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let want = q.count.max(1);
    let timeout = parse_duration(&q.timeout);
    let start_count = state.store.message_count(id);
    if start_count >= want {
        return Ok(Json(MessagesOut {
            messages: state.store.list_messages(id),
            timed_out: None,
        }));
    }

    let mut rx = state.store.subscribe();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(Json(MessagesOut {
                messages: state.store.list_messages(id),
                timed_out: Some(true),
            }));
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(StoreEvent::MessageCaptured { channel_id, .. })) if channel_id == id => {
                if state.store.message_count(id) >= want {
                    return Ok(Json(MessagesOut {
                        messages: state.store.list_messages(id),
                        timed_out: None,
                    }));
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {
                // lag or closed — re-subscribe and check count
                rx = state.store.subscribe();
                if state.store.message_count(id) >= want {
                    return Ok(Json(MessagesOut {
                        messages: state.store.list_messages(id),
                        timed_out: None,
                    }));
                }
            }
            Err(_) => {
                return Ok(Json(MessagesOut {
                    messages: state.store.list_messages(id),
                    timed_out: Some(true),
                }));
            }
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/channels/{id}/faults",
    tag = "channels",
    params(("id" = Uuid, Path, description = "Channel id")),
    request_body = FaultProfile,
    responses(
        (status = 200, description = "Updated channel", body = ChannelOut),
        (status = 404, description = "Not found")
    )
)]
async fn set_faults(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(faults): Json<FaultProfile>,
) -> Result<Json<ChannelOut>, StatusCode> {
    state
        .store
        .update_faults(id, faults)
        .map(|c| Json(channel_out(&state, c)))
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Deserialize, ToSchema)]
struct ArchiveBody {
    archived: bool,
}

#[utoipa::path(
    post,
    path = "/api/channels/{id}/archive",
    tag = "channels",
    params(("id" = Uuid, Path, description = "Channel id")),
    request_body = ArchiveBody,
    responses(
        (status = 200, description = "Updated channel", body = ChannelOut),
        (status = 404, description = "Not found")
    )
)]
async fn set_archive(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ArchiveBody>,
) -> Result<Json<ChannelOut>, StatusCode> {
    state
        .store
        .set_archived(id, body.archived)
        .map(|c| Json(channel_out(&state, c)))
        .ok_or(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    get,
    path = "/api/events",
    tag = "events",
    responses(
        (status = 200, description = "Server-sent events stream", content_type = "text/event-stream")
    )
)]
async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.store.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|item| {
        let event = match item {
            Ok(StoreEvent::ChannelCreated(ch)) => Event::default()
                .event("channel_created")
                .data(serde_json::to_string(&ch).unwrap_or_default()),
            Ok(StoreEvent::ChannelUpdated(ch)) => Event::default()
                .event("channel_updated")
                .data(serde_json::to_string(&ch).unwrap_or_default()),
            Ok(StoreEvent::ChannelDeleted { id }) => Event::default()
                .event("channel_deleted")
                .data(serde_json::json!({ "id": id }).to_string()),
            Ok(StoreEvent::MessageCaptured {
                channel_id,
                capture,
            }) => Event::default().event("message").data(
                serde_json::json!({
                    "channel_id": channel_id,
                    "id": capture.id,
                    "summary": capture.rendered.summary(),
                    "received_at": capture.received_at.format(&Rfc3339).unwrap_or_default(),
                    "status": capture.response.status,
                    "warnings": capture.warnings.len(),
                })
                .to_string(),
            ),
            Ok(StoreEvent::MessagesCleared { channel_id }) => Event::default()
                .event("messages_cleared")
                .data(serde_json::json!({ "channel_id": channel_id }).to_string()),
            Err(_) => return None,
        };
        Some(Ok(event))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[utoipa::path(
    get,
    path = "/healthz",
    tag = "health",
    responses(
        (status = 200, description = "OK", body = String, content_type = "text/plain")
    )
)]
pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
