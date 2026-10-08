//! HTTP contract tests for the README examples and provider table.
//! Prefer extending this file when README curls / URL shapes change.

use std::time::Duration;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use webhook_mocker::{AppState, build_router, store::Store};

fn app() -> axum::Router {
    build_router(
        AppState {
            store: Store::new(100, None),
            public_base: "http://127.0.0.1:5080".into(),
            strict: false,
            auto_create: true,
        },
        1_048_576,
    )
}

fn app_no_auto_create() -> axum::Router {
    build_router(
        AppState {
            store: Store::new(100, None),
            public_base: "http://127.0.0.1:5080".into(),
            strict: false,
            auto_create: false,
        },
        1_048_576,
    )
}

async fn call(
    app: axum::Router,
    req: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn json_body(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("invalid json ({e}): {body}"))
}

async fn create_channel(app: axum::Router, kind: &str, name: &str) -> (axum::Router, Value) {
    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/api/channels")
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"kind":"{kind}","name":"{name}"}}"#
            )))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create channel: {body}");
    let v = json_body(&body).await;
    (app, v)
}

#[tokio::test]
async fn healthz_ok() {
    let (status, _, body) = call(
        app(),
        Request::builder()
            .uri("/healthz")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}

// --- Provider table (README Quick start) ---

#[tokio::test]
async fn slack_readme_text_returns_ok() {
    // README: POST /slack/services/{T}/{B}/{token} → 200 body `ok`
    let (status, _, body) = call(
        app(),
        Request::builder()
            .method("POST")
            .uri("/slack/services/T0/B0/dev")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"text":"Hello from Slack"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn slack_blocks_fixture_returns_ok_and_lists_channel() {
    let app = app();
    let fixture = include_str!("fixtures/slack_blocks.json");
    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/slack/services/TFIX/BFIX/fixturetoken")
            .header("content-type", "application/json")
            .body(Body::from(fixture))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");

    let (status, _, body) = call(
        app,
        Request::builder()
            .uri("/api/channels")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("fixturetoken") || body.contains("slack"),
        "channel list should mention the auto-created slack channel: {body}"
    );
}

#[tokio::test]
async fn teams_workflows_readme_minimal_accepted() {
    // README example Adaptive Card payload → 202 Accepted
    let payload = r#"{"type":"message","attachments":[{"contentType":"application/vnd.microsoft.card.adaptive","content":{"type":"AdaptiveCard","version":"1.4","body":[{"type":"TextBlock","text":"Hello from Teams"}]}}]}"#;
    let (status, _, _) = call(
        app(),
        Request::builder()
            .method("POST")
            .uri("/teams/workflows/dev")
            .header("content-type", "application/json")
            .body(Body::from(payload))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn teams_workflows_fixture_accepted() {
    let fixture = include_str!("fixtures/teams_adaptive.json");
    let (status, _, _) = call(
        app(),
        Request::builder()
            .method("POST")
            .uri("/teams/workflows/wf-fixture")
            .header("content-type", "application/json")
            .body(Body::from(fixture))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn teams_legacy_returns_one() {
    let fixture = include_str!("fixtures/teams_messagecard.json");
    let (status, _, body) = call(
        app(),
        Request::builder()
            .method("POST")
            .uri("/teams/webhookb2/legacy-fixture")
            .header("content-type", "application/json")
            .body(Body::from(fixture))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "1");
}

#[tokio::test]
async fn generic_hook_post_and_get_return_ok_json() {
    // README: ANY /hooks/{id} → 200 {"ok":true}
    let app = app();
    let (status, headers, body) = call(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/hooks/readme-generic")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"text":"hi"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json_body(&body).await, serde_json::json!({"ok": true}));
    assert!(
        headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("application/json")),
        "expected json content-type, got {headers:?}"
    );

    let (status, _, body) = call(
        app,
        Request::builder()
            .method("GET")
            .uri("/hooks/readme-generic")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json_body(&body).await, serde_json::json!({"ok": true}));
}

#[tokio::test]
async fn generic_hook_404_when_auto_create_disabled() {
    let (status, _, _) = call(
        app_no_auto_create(),
        Request::builder()
            .method("POST")
            .uri("/hooks/missing")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// --- HTTP API (README curl block) ---

#[tokio::test]
async fn readme_api_list_create_messages_clear_wait_faults() {
    let app = app();

    // List (empty)
    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .uri("/api/channels")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json_body(&body).await, serde_json::json!([]));

    // Create — README uses kind slack / name alerts
    let (app, ch) = create_channel(app, "slack", "alerts").await;
    let id = ch["id"].as_str().expect("id");
    let webhook_path = ch["webhook_path"].as_str().expect("webhook_path");
    assert!(webhook_path.starts_with("/slack/services/"));
    assert!(ch["webhook_url"]
        .as_str()
        .unwrap()
        .starts_with("http://127.0.0.1:5080/"));

    // Wait with nothing yet → timed_out (short timeout)
    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .uri(format!("/api/channels/{id}/wait?count=1&timeout=50ms"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let wait = json_body(&body).await;
    assert_eq!(wait["timed_out"], true);
    assert_eq!(wait["messages"].as_array().unwrap().len(), 0);

    // Deliver a webhook into the channel
    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri(webhook_path)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"text":"captured"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");

    // List messages
    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .uri(format!("/api/channels/{id}/messages"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let msgs = json_body(&body).await;
    assert_eq!(msgs["messages"].as_array().unwrap().len(), 1);
    let msg = &msgs["messages"][0];
    assert!(
        msg["body_utf8"]
            .as_str()
            .unwrap_or("")
            .contains("captured")
            || msg["body_json"]["text"].as_str() == Some("captured"),
        "message should include payload: {msg}"
    );

    // Wait when count already satisfied
    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .uri(format!("/api/channels/{id}/wait?count=1&timeout=5s"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let wait = json_body(&body).await;
    assert!(wait.get("timed_out").is_none() || wait["timed_out"].is_null());
    assert_eq!(wait["messages"].as_array().unwrap().len(), 1);

    // Clear
    let (status, _, _) = call(
        app.clone(),
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/channels/{id}/messages"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .uri(format!("/api/channels/{id}/messages"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json_body(&body).await["messages"].as_array().unwrap().len(),
        0
    );

    // Fault injection — README body
    let (status, _, _) = call(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri(format!("/api/channels/{id}/faults"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"force_429":true,"retry_after_secs":2}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, headers, _) = call(
        app,
        Request::builder()
            .method("POST")
            .uri(webhook_path)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"text":"should-429"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        headers.get("retry-after").and_then(|v| v.to_str().ok()),
        Some("2")
    );
}

#[tokio::test]
async fn readme_api_events_sse_content_type() {
    let (status, headers, _) = call(
        app(),
        Request::builder()
            .uri("/api/events")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ct = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.starts_with("text/event-stream"),
        "expected text/event-stream, got {ct}"
    );
}

#[tokio::test]
async fn events_sse_emits_channel_created() {
    let app = app();
    let sse_app = app.clone();
    let create_app = app.clone();

    let sse = tokio::spawn(async move {
        let res = sse_app
            .oneshot(
                Request::builder()
                    .uri("/api/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let mut body = res.into_body();
        let mut buf = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, body.frame()).await {
                Ok(Some(Ok(frame))) => {
                    if let Ok(data) = frame.into_data() {
                        buf.extend_from_slice(&data);
                        let text = String::from_utf8_lossy(&buf);
                        if text.contains("channel_created") {
                            return text.into_owned();
                        }
                    }
                }
                Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    });

    // Let the SSE handler subscribe before creating the channel.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (status, _, _) = call(
        create_app,
        Request::builder()
            .method("POST")
            .uri("/api/channels")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"kind":"generic","name":"sse-probe"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let text = sse.await.unwrap();
    assert!(
        text.contains("channel_created"),
        "SSE stream should include channel_created event, got: {text}"
    );
}

#[tokio::test]
async fn create_generic_channel_api() {
    let (_, ch) = create_channel(app(), "generic", "catch-all").await;
    assert_eq!(ch["name"], "catch-all");
    assert!(
        ch["webhook_path"]
            .as_str()
            .unwrap_or("")
            .starts_with("/hooks/")
    );
}

#[tokio::test]
async fn ui_index_renders() {
    let (status, _, body) = call(
        app(),
        Request::builder().uri("/").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("webhook-mocker"));
    assert!(body.contains("DEV ONLY"));
}

#[tokio::test]
async fn openapi_and_swagger_ui_served() {
    let app = app();
    let (status, _, body) = call(
        app.clone(),
        Request::builder()
            .uri("/api-docs/openapi.json")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"openapi\""));
    assert!(body.contains("/api/channels"));
    assert!(body.contains("webhook-mocker"));

    let (status, _, body) = call(
        app,
        Request::builder()
            .uri("/swagger-ui/")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.to_lowercase().contains("swagger"),
        "swagger UI html expected, got: {}",
        &body[..body.len().min(200)]
    );
}
