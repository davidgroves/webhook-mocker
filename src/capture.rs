use std::time::Instant;

use axum::{
    body::Body,
    extract::{ConnectInfo, Request},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    render::RenderedMessage,
    store::{Capture, CaptureResponse, Channel},
};

pub struct IncomingRequest {
    pub remote_addr: Option<String>,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    pub started: Instant,
}

impl IncomingRequest {
    #[allow(clippy::result_large_err)] // axum Response is the idiomatic error type here
    pub async fn from_request(req: Request) -> Result<Self, Response> {
        let remote_addr = req
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|c| c.0.to_string());
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        let query = req.uri().query().map(str::to_string);
        let headers = headers_to_vec(req.headers());
        let started = Instant::now();
        let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
            Ok(b) => b,
            Err(err) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("failed to read body: {err}"),
                )
                    .into_response());
            }
        };
        Ok(Self {
            remote_addr,
            method,
            path,
            query,
            headers,
            body,
            started,
        })
    }

    pub fn body_utf8(&self) -> Option<String> {
        std::str::from_utf8(&self.body).ok().map(str::to_string)
    }

    pub fn body_json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }

    pub fn parse_slack_body(&self) -> Result<serde_json::Value, String> {
        let content_type = self
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");

        if content_type.contains("application/x-www-form-urlencoded") {
            let form: std::collections::HashMap<String, String> =
                serde_urlencoded::from_bytes(&self.body).map_err(|e| e.to_string())?;
            if let Some(payload) = form.get("payload") {
                return serde_json::from_str(payload).map_err(|e| e.to_string());
            }
            // Some clients send text= directly
            if let Some(text) = form.get("text") {
                return Ok(serde_json::json!({ "text": text }));
            }
            return Err("form body missing payload=".into());
        }

        serde_json::from_slice(&self.body).map_err(|e| e.to_string())
    }

    pub fn into_capture(
        self,
        channel: &Channel,
        warnings: Vec<String>,
        rendered: RenderedMessage,
        response: CaptureResponse,
    ) -> Capture {
        let body_utf8 = self.body_utf8();
        let body_json = self.body_json();
        Capture {
            id: Uuid::new_v4(),
            channel_id: channel.id,
            received_at: OffsetDateTime::now_utc(),
            remote_addr: self.remote_addr,
            method: self.method,
            path: self.path,
            query: self.query,
            headers: self.headers,
            body_bytes: self.body.to_vec(),
            body_utf8,
            body_json,
            warnings,
            rendered,
            response,
        }
    }
}

pub fn headers_to_vec(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                v.to_str().unwrap_or("<non-utf8>").to_string(),
            )
        })
        .collect()
}

pub fn response_meta(
    status: StatusCode,
    headers: Vec<(String, String)>,
    body: impl Into<String>,
    started: Instant,
) -> (CaptureResponse, Response) {
    let body = body.into();
    let meta = CaptureResponse {
        status: status.as_u16(),
        headers: headers.clone(),
        body: body.clone(),
        duration_ms: started.elapsed().as_millis() as u64,
    };
    let mut builder = Response::builder().status(status);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let resp = builder
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    (meta, resp)
}
