use askama::Template;
use axum::{
    Router,
    extract::{Path, State},
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use rust_embed::Embed;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::{
    AppState,
    render::RenderBlock,
    store::{Capture, Channel, ChannelKind},
};

#[derive(Embed)]
#[folder = "assets/"]
struct Assets;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/ui/channel/{id}", get(channel_page))
        .route("/ui/channel/{id}/message/{mid}", get(message_partial))
        .route("/assets/{*path}", get(static_asset))
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    public_base: String,
    channels: Vec<ChannelView>,
    selected: Option<ChannelView>,
    messages: Vec<MessageView>,
    selected_message: Option<MessageDetailView>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct ChannelView {
    id: String,
    name: String,
    kind: String,
    kind_label: String,
    webhook_url: String,
    unread: usize,
    message_count: usize,
    archived: bool,
    active: bool,
}

#[derive(Clone)]
struct MessageView {
    id: String,
    summary: String,
    received_at: String,
    status: u16,
    warning_count: usize,
    active: bool,
}

#[allow(dead_code)]
struct MessageDetailView {
    id: String,
    received_at: String,
    method: String,
    path: String,
    query: String,
    remote_addr: String,
    status: u16,
    duration_ms: u64,
    response_body: String,
    warnings: Vec<String>,
    headers: Vec<(String, String)>,
    response_headers: Vec<(String, String)>,
    body_utf8: String,
    body_json_pretty: String,
    body_hex: String,
    curl: String,
    username: String,
    blocks_html: String,
    kind_class: String,
}

fn channel_view(state: &AppState, ch: &Channel, active: bool) -> ChannelView {
    ChannelView {
        id: ch.id.to_string(),
        name: ch.name.clone(),
        kind: ch.kind.as_str().to_string(),
        kind_label: ch.kind.label().to_string(),
        webhook_url: ch.webhook_url(&state.public_base),
        unread: ch.unread,
        message_count: state.store.message_count(ch.id),
        archived: ch.archived,
        active,
    }
}

fn message_view(c: &Capture, active: bool) -> MessageView {
    MessageView {
        id: c.id.to_string(),
        summary: c.rendered.summary(),
        received_at: c.received_at.format(&Rfc3339).unwrap_or_default(),
        status: c.response.status,
        warning_count: c.warnings.len(),
        active,
    }
}

fn blocks_to_html(blocks: &[RenderBlock]) -> String {
    let mut html = String::new();
    for b in blocks {
        match b {
            RenderBlock::Header { text } => {
                html.push_str(&format!("<h3 class=\"blk-header\">{}</h3>", esc(text)));
            }
            RenderBlock::Text { html: h, .. } => {
                html.push_str(&format!("<div class=\"blk-text\">{h}</div>"));
            }
            RenderBlock::Fields { fields } | RenderBlock::FactSet { facts: fields } => {
                html.push_str("<div class=\"blk-fields\">");
                for f in fields {
                    html.push_str(&format!(
                        "<div class=\"field\"><div class=\"field-title\">{}</div><div class=\"field-value\">{}</div></div>",
                        esc(&f.title),
                        f.value_html
                    ));
                }
                html.push_str("</div>");
            }
            RenderBlock::Image { url, alt } => {
                html.push_str(&format!(
                    r#"<img class="blk-image" src="{}" alt="{}" loading="lazy">"#,
                    esc(url),
                    esc(alt.as_deref().unwrap_or(""))
                ));
            }
            RenderBlock::Divider => html.push_str("<hr class=\"blk-divider\">"),
            RenderBlock::Actions { actions } => {
                html.push_str("<div class=\"blk-actions\">");
                for a in actions {
                    let label = esc(&a.label);
                    if let Some(url) = &a.url {
                        html.push_str(&format!(
                            r#"<a class="btn {}" href="{}" target="_blank" rel="noopener">{}</a>"#,
                            a.style.as_deref().unwrap_or("default"),
                            esc(url),
                            label
                        ));
                    } else {
                        html.push_str(&format!(
                            r#"<button class="btn {}" type="button" disabled>{}</button>"#,
                            a.style.as_deref().unwrap_or("default"),
                            label
                        ));
                    }
                }
                html.push_str("</div>");
            }
            RenderBlock::Context { elements } => {
                html.push_str("<div class=\"blk-context\">");
                for e in elements {
                    html.push_str(&format!("<span>{}</span>", esc(e)));
                }
                html.push_str("</div>");
            }
            RenderBlock::Columns { columns } => {
                html.push_str("<div class=\"blk-columns\">");
                for col in columns {
                    html.push_str("<div class=\"blk-column\">");
                    html.push_str(&blocks_to_html(col));
                    html.push_str("</div>");
                }
                html.push_str("</div>");
            }
            RenderBlock::Unsupported { kind, raw } => {
                html.push_str(&format!(
                    r#"<div class="blk-unsupported"><span class="chip">{}</span><pre>{}</pre></div>"#,
                    esc(kind),
                    esc(raw)
                ));
            }
            RenderBlock::Code { language, text } => {
                html.push_str(&format!(
                    "<pre class=\"blk-code\" data-lang=\"{}\"><code>{}</code></pre>",
                    esc(language.as_deref().unwrap_or("")),
                    esc(text)
                ));
            }
        }
    }
    html
}

fn esc(s: &str) -> String {
    crate::render::html_escape(s)
}

fn detail_view(state: &AppState, ch: &Channel, c: &Capture) -> MessageDetailView {
    MessageDetailView {
        id: c.id.to_string(),
        received_at: c.received_at.format(&Rfc3339).unwrap_or_default(),
        method: c.method.clone(),
        path: c.path.clone(),
        query: c.query.clone().unwrap_or_default(),
        remote_addr: c.remote_addr.clone().unwrap_or_else(|| "—".into()),
        status: c.response.status,
        duration_ms: c.response.duration_ms,
        response_body: c.response.body.clone(),
        warnings: c.warnings.clone(),
        headers: c.headers.clone(),
        response_headers: c.response.headers.clone(),
        body_utf8: c
            .body_utf8
            .clone()
            .unwrap_or_else(|| format!("(binary, {} bytes)\n{}", c.body_bytes.len(), c.body_hex())),
        body_json_pretty: c
            .body_json
            .as_ref()
            .and_then(|v| serde_json::to_string_pretty(v).ok())
            .unwrap_or_default(),
        body_hex: c.body_hex(),
        curl: c.curl(&state.public_base),
        username: c
            .rendered
            .username
            .clone()
            .or_else(|| c.rendered.icon_emoji.clone())
            .unwrap_or_else(|| ch.kind.label().into()),
        blocks_html: blocks_to_html(&c.rendered.blocks),
        kind_class: match ch.kind {
            ChannelKind::Slack => "theme-slack",
            ChannelKind::TeamsWorkflows | ChannelKind::TeamsLegacy => "theme-teams",
            ChannelKind::Generic => "theme-generic",
        }
        .into(),
    }
}

async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let channels = state.store.list_channels();
    let selected = channels.first().cloned();
    let selected_id = selected.as_ref().map(|c| c.id);
    let messages = selected_id
        .map(|id| {
            state.store.mark_read(id);
            state.store.list_messages(id)
        })
        .unwrap_or_default();
    let selected_message = messages.last().cloned();
    let msg_id = selected_message.as_ref().map(|m| m.id);

    let tpl = IndexTemplate {
        public_base: state.public_base.clone(),
        channels: channels
            .iter()
            .map(|c| channel_view(&state, c, Some(c.id) == selected_id))
            .collect(),
        selected: selected.as_ref().map(|c| channel_view(&state, c, true)),
        messages: messages
            .iter()
            .rev()
            .map(|m| message_view(m, Some(m.id) == msg_id))
            .collect(),
        selected_message: selected.as_ref().and_then(|ch| {
            selected_message
                .as_ref()
                .map(|m| detail_view(&state, ch, m))
        }),
    };
    tpl.render()
        .map(Html)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn channel_page(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, StatusCode> {
    let channels = state.store.list_channels();
    let selected = state.store.get_channel(id).ok_or(StatusCode::NOT_FOUND)?;
    state.store.mark_read(id);
    let messages = state.store.list_messages(id);
    let selected_message = messages.last().cloned();
    let msg_id = selected_message.as_ref().map(|m| m.id);

    let tpl = IndexTemplate {
        public_base: state.public_base.clone(),
        channels: channels
            .iter()
            .map(|c| channel_view(&state, c, c.id == id))
            .collect(),
        selected: Some(channel_view(&state, &selected, true)),
        messages: messages
            .iter()
            .rev()
            .map(|m| message_view(m, Some(m.id) == msg_id))
            .collect(),
        selected_message: selected_message
            .as_ref()
            .map(|m| detail_view(&state, &selected, m)),
    };
    tpl.render()
        .map(Html)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn message_partial(
    State(state): State<AppState>,
    Path((id, mid)): Path<(Uuid, Uuid)>,
) -> Result<Html<String>, StatusCode> {
    let ch = state.store.get_channel(id).ok_or(StatusCode::NOT_FOUND)?;
    let msg = state
        .store
        .get_message(id, mid)
        .ok_or(StatusCode::NOT_FOUND)?;
    let channels = state.store.list_channels();
    let messages = state.store.list_messages(id);

    let tpl = IndexTemplate {
        public_base: state.public_base.clone(),
        channels: channels
            .iter()
            .map(|c| channel_view(&state, c, c.id == id))
            .collect(),
        selected: Some(channel_view(&state, &ch, true)),
        messages: messages
            .iter()
            .rev()
            .map(|m| message_view(m, m.id == mid))
            .collect(),
        selected_message: Some(detail_view(&state, &ch, &msg)),
    };
    tpl.render()
        .map(Html)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn static_asset(Path(path): Path<String>) -> Response {
    match Assets::get(&path) {
        Some(file) => {
            let mime = mime_guess(&path);
            ([(header::CONTENT_TYPE, mime)], file.data).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn mime_guess(path: &str) -> &'static str {
    if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

/// Redirect bare unknown paths? unused helper kept for future.
#[allow(dead_code)]
async fn not_found(uri: Uri) -> impl IntoResponse {
    (StatusCode::NOT_FOUND, format!("not found: {uri}"))
}
