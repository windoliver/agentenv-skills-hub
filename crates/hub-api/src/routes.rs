use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::handlers;

pub fn build_router() -> Router {
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
        .route("/api/v1/skills", get(handlers::healthz))
        .route("/api/v1/skills/{namespace}/{name}", get(handlers::healthz))
        .route(
            "/api/v1/skills/{namespace}/{name}/versions",
            get(handlers::healthz),
        )
        .route(
            "/api/v1/skills/{namespace}/{name}/versions",
            post(handlers::healthz),
        )
        .route(
            "/api/v1/skills/{namespace}/{name}/versions/{version}",
            get(handlers::healthz),
        )
        .route(
            "/api/v1/skills/{namespace}/{name}/versions/{version}/yank",
            post(handlers::healthz),
        )
        .route(
            "/api/v1/skills/{namespace}/{name}/versions/{version}/unyank",
            post(handlers::healthz),
        )
        .route("/api/v1/search", get(handlers::healthz))
        .route("/api/v1/search/similar", post(handlers::healthz))
        .route(
            "/api/v1/webhooks",
            get(handlers::healthz).post(handlers::healthz),
        )
        .route("/api/v1/webhooks/{id}", delete(handlers::healthz))
}
