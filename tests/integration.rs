use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
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

async fn body_string(res: axum::response::Response) -> String {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn healthz_ok() {
    let res = app()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn slack_fixture_and_api() {
    let app = app();
    let fixture = include_str!("fixtures/slack_blocks.json");
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/slack/services/TFIX/BFIX/fixturetoken")
                .header("content-type", "application/json")
                .body(Body::from(fixture))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_string(res).await, "ok");

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/channels")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;
    assert!(body.contains("fixturetoken") || body.contains("slack"));
}

#[tokio::test]
async fn teams_workflows_accepted() {
    let fixture = include_str!("fixtures/teams_adaptive.json");
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/teams/workflows/wf-fixture")
                .header("content-type", "application/json")
                .body(Body::from(fixture))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn teams_legacy_returns_one() {
    let fixture = include_str!("fixtures/teams_messagecard.json");
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/teams/webhookb2/legacy-fixture")
                .header("content-type", "application/json")
                .body(Body::from(fixture))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_string(res).await, "1");
}

#[tokio::test]
async fn create_channel_api() {
    let res = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/channels")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"kind":"generic","name":"catch-all"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_string(res).await;
    assert!(body.contains("catch-all"));
    assert!(body.contains("/hooks/"));
}

#[tokio::test]
async fn ui_index_renders() {
    let res = app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;
    assert!(body.contains("webhook-mocker"));
    assert!(body.contains("DEV ONLY"));
}

#[tokio::test]
async fn openapi_spec_served() {
    let res = app()
        .oneshot(
            Request::builder()
                .uri("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;
    assert!(body.contains("\"openapi\""));
    assert!(body.contains("/api/channels"));
    assert!(body.contains("webhook-mocker"));
}
