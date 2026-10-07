pub mod api;
pub mod capture;
pub mod config;
pub mod faults;
pub mod providers;
pub mod render;
pub mod store;
pub mod ui;

use axum::{Router, routing::get};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

use crate::store::Store;

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub public_base: String,
    pub strict: bool,
    pub auto_create: bool,
}

pub fn build_router(state: AppState, max_body: usize) -> Router {
    Router::new()
        .route("/healthz", get(api::healthz))
        .merge(api::router())
        .merge(providers::router())
        .merge(ui::router())
        .merge(api::docs_router())
        .layer(RequestBodyLimitLayer::new(max_body))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
