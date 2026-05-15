use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use hub_api::{
    routes::{build_router, build_router_with_state},
    state::{AppState, RuntimeArtifactStore, RuntimeTrustVerifier, RuntimeWebhookQueue},
};
use hub_index::PgHubRepository;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tower::ServiceExt;

#[tokio::test]
async fn mcp_initialize_returns_tool_capabilities() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "contract-test", "version": "0.1.0"}
        }
    }))
    .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(response["result"]["capabilities"]["tools"], json!({}));
    assert_eq!(
        response["result"]["serverInfo"],
        json!({"name": "agentenv-skills-hub", "version": env!("CARGO_PKG_VERSION")})
    );
}

#[tokio::test]
async fn mcp_tools_list_exposes_only_read_only_skill_tools() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "tools",
        "method": "tools/list",
        "params": {}
    }))
    .await;

    let tools = response["result"]["tools"].as_array().unwrap();
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "skills.search",
            "skills.find_similar",
            "skills.get_manifest",
            "skills.suggest_for_task"
        ]
    );
    assert!(tools
        .iter()
        .all(|tool| tool["inputSchema"]["type"] == "object"));
}

#[tokio::test]
async fn mcp_skills_search_returns_fixture_skill_summaries() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "skills.search",
            "arguments": {"query": "review", "limit": 5}
        }
    }))
    .await;

    let result = response["result"].clone();
    assert_eq!(result["isError"], false);
    let payload = tool_json_payload(&result);
    assert_eq!(payload["warnings"], json!([]));
    assert_eq!(payload["skills"][0]["name"], "code-review");
    assert_eq!(payload["skills"][0]["version"], "1.2.0");
    assert_eq!(payload["skills"][0]["registry"], "community");
}

#[tokio::test]
async fn mcp_get_manifest_returns_exact_fixture_manifest() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "manifest-exact",
        "method": "tools/call",
        "params": {
            "name": "skills.get_manifest",
            "arguments": {"name": "code-review", "version": "1.2.0"}
        }
    }))
    .await;

    let result = response["result"].clone();
    assert_eq!(result["isError"], false);
    let payload = tool_json_payload(&result);
    let manifest = &payload["manifest"];
    assert_eq!(manifest["name"], "code-review");
    assert_eq!(manifest["version"], "1.2.0");
    assert_eq!(manifest["entry"], "SKILL.md");
    assert_eq!(manifest["files"], json!(["SKILL.md"]));
}

#[tokio::test]
async fn mcp_get_manifest_omitted_version_uses_latest_visible_fixture_version() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "manifest-latest",
        "method": "tools/call",
        "params": {
            "name": "skills.get_manifest",
            "arguments": {"name": "code-review"}
        }
    }))
    .await;

    let result = response["result"].clone();
    assert_eq!(result["isError"], false);
    let payload = tool_json_payload(&result);
    let manifest = &payload["manifest"];
    assert_eq!(manifest["name"], "code-review");
    assert_eq!(manifest["version"], "1.2.0");
    assert_eq!(manifest["entry"], "SKILL.md");
    assert_eq!(manifest["files"], json!(["SKILL.md"]));
}

#[tokio::test]
async fn mcp_get_manifest_unknown_fixture_skill_returns_tool_error() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "manifest-missing",
        "method": "tools/call",
        "params": {
            "name": "skills.get_manifest",
            "arguments": {"name": "missing-skill", "version": "1.2.0"}
        }
    }))
    .await;

    let result = response["result"].clone();
    assert_eq!(result["isError"], true);
    assert_eq!(
        tool_json_payload(&result),
        json!({"error": "skill manifest was not found"})
    );
}

#[tokio::test]
async fn mcp_get_manifest_internal_lookup_failure_returns_json_rpc_error() {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://agentenv:agentenv@127.0.0.1:1/agentenv")
        .unwrap();
    let state = AppState::with_repository(
        PgHubRepository::new(pool),
        "community",
        true,
        RuntimeArtifactStore::default(),
        RuntimeTrustVerifier,
        RuntimeWebhookQueue::default(),
        None,
    );

    let response = mcp_request_with_app(
        build_router_with_state(state),
        json!({
            "jsonrpc": "2.0",
            "id": "manifest-db-error",
            "method": "tools/call",
            "params": {
                "name": "skills.get_manifest",
                "arguments": {"name": "code-review", "version": "1.2.0"}
            }
        }),
    )
    .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], "manifest-db-error");
    assert_eq!(response["error"]["code"], -32603);
    assert_eq!(response["error"]["message"], "skill manifest lookup failed");
    assert!(response.get("result").is_none());
}

#[tokio::test]
async fn mcp_find_similar_reports_semantic_unavailable_without_backend() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "similar",
        "method": "tools/call",
        "params": {
            "name": "skills.find_similar",
            "arguments": {
                "description": "Review code changes and produce actionable comments",
                "limit": 5
            }
        }
    }))
    .await;

    assert_eq!(response["result"]["isError"], true);
    let payload = tool_json_payload(&response["result"]);
    assert_eq!(payload["error"], "semantic search is not configured");
}

#[tokio::test]
async fn mcp_suggest_for_task_falls_back_to_lexical_search_with_warning() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "suggest",
        "method": "tools/call",
        "params": {
            "name": "skills.suggest_for_task",
            "arguments": {
                "task_description": "review code changes",
                "limit": 5
            }
        }
    }))
    .await;

    assert_eq!(response["result"]["isError"], false);
    let payload = tool_json_payload(&response["result"]);
    assert_eq!(payload["skills"][0]["name"], "code-review");
    assert_eq!(
        payload["warnings"],
        json!(["semantic search is not configured; used lexical fallback"])
    );
}

#[tokio::test]
async fn mcp_unknown_method_returns_method_not_found() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "unknown-method",
        "method": "missing/method",
        "params": {}
    }))
    .await;

    assert_eq!(response["error"]["code"], -32601);
}

#[tokio::test]
async fn mcp_unknown_tool_returns_invalid_params() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "unknown-tool",
        "method": "tools/call",
        "params": {"name": "skills.publish", "arguments": {}}
    }))
    .await;

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "unknown tool");
}

#[tokio::test]
async fn mcp_rejects_empty_search_query() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "empty-search",
        "method": "tools/call",
        "params": {
            "name": "skills.search",
            "arguments": {"query": "   "}
        }
    }))
    .await;

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "`query` must not be empty");
}

#[tokio::test]
async fn mcp_rejects_zero_limit() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "zero-limit",
        "method": "tools/call",
        "params": {
            "name": "skills.search",
            "arguments": {"query": "review", "limit": 0}
        }
    }))
    .await;

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(
        response["error"]["message"],
        "`limit` must be greater than zero"
    );
}

#[tokio::test]
async fn mcp_clamps_large_search_limit() {
    let response = mcp_request(json!({
        "jsonrpc": "2.0",
        "id": "large-limit",
        "method": "tools/call",
        "params": {
            "name": "skills.search",
            "arguments": {"query": "review", "limit": 500}
        }
    }))
    .await;

    let payload = tool_json_payload(&response["result"]);
    assert_eq!(payload["skills"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn mcp_malformed_json_returns_parse_error() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{not-json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["id"], Value::Null);
    assert_eq!(body["error"]["code"], -32700);
}

async fn mcp_request(payload: Value) -> Value {
    mcp_request_with_app(build_router(), payload).await
}

async fn mcp_request_with_app(app: axum::Router, payload: Value) -> Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

fn tool_json_payload(result: &Value) -> Value {
    let text = result["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}
