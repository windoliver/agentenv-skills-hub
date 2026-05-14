use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::{handlers, state::AppState};

pub fn build_router() -> Router {
    build_router_with_state(AppState::fixture())
}

pub fn build_router_with_state(state: AppState) -> Router {
    Router::new()
        .route(
            "/.well-known/agent-skills",
            get(handlers::well_known_agent_skills),
        )
        .route("/index.json", get(handlers::index_json))
        .route("/skills/{name}/{artifact}", get(handlers::fixture_artifact))
        .route("/api/v1/healthz", get(handlers::healthz))
        .route("/api/v1/readyz", get(handlers::readyz))
        .route("/metrics", get(handlers::metrics))
        .route("/api/v1/skills", get(handlers::list_skills))
        .route(
            "/api/v1/skills/{namespace}/{name}",
            get(handlers::get_skill),
        )
        .route(
            "/api/v1/skills/{namespace}/{name}/versions",
            get(handlers::list_versions).post(handlers::publish_version),
        )
        .route(
            "/api/v1/skills/{namespace}/{name}/versions/{version}",
            get(handlers::get_version),
        )
        .route(
            "/api/v1/skills/{namespace}/{name}/versions/{version}/yank",
            post(handlers::yank_version),
        )
        .route(
            "/api/v1/skills/{namespace}/{name}/versions/{version}/unyank",
            post(handlers::unyank_version),
        )
        .route("/api/v1/search", get(handlers::search))
        .route("/api/v1/search/similar", post(handlers::similar_search))
        .route(
            "/api/v1/webhooks",
            get(handlers::list_webhooks).post(handlers::create_webhook),
        )
        .route("/api/v1/webhooks/{id}", delete(handlers::delete_webhook))
        .with_state(state)
}
