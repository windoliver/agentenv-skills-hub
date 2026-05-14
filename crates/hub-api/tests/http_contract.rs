use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use hub_api::routes::build_router;
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_ok() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn well_known_agent_skills_returns_discovery_document() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/agent-skills")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn index_json_returns_agentenv_compatible_shape() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/index.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
