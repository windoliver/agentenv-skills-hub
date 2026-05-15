use axum::{body::Bytes, extract::State, http::StatusCode, response::IntoResponse, Json};
use hub_core::validation::{validate_skill_name, validate_version};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    read_model::{filtered_search_index, manifest_for_state, SearchParams},
    state::AppState,
};

const JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MAX_LIMIT: usize = 50;
const DEFAULT_LIMIT: usize = 20;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: Option<String>,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Deserialize)]
struct SearchArgs {
    query: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct FindSimilarArgs {
    description: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct GetManifestArgs {
    name: String,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SuggestForTaskArgs {
    task_description: String,
    limit: Option<usize>,
}

pub async fn mcp_endpoint(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let request = match serde_json::from_slice::<JsonRpcRequest>(&body) {
        Ok(request) => request,
        Err(_) => {
            return (
                StatusCode::OK,
                Json(error_response(Value::Null, PARSE_ERROR, "parse error")),
            );
        }
    };

    let id = request.id.clone().unwrap_or(Value::Null);
    let response = match handle_request(&state, request).await {
        Ok(result) => success_response(id, result),
        Err(error) => error_response(id, error.code, error.message),
    };

    (StatusCode::OK, Json(response))
}

async fn handle_request(state: &AppState, request: JsonRpcRequest) -> Result<Value, McpError> {
    if request.jsonrpc.as_deref() != Some(JSONRPC_VERSION) {
        return Err(McpError::new(INVALID_REQUEST, "invalid JSON-RPC request"));
    }
    let method = request
        .method
        .as_deref()
        .ok_or_else(|| McpError::new(INVALID_REQUEST, "missing JSON-RPC method"))?;
    match method {
        "initialize" => Ok(initialize_result()),
        "tools/list" => Ok(tools_list_result()),
        "tools/call" => tools_call(state, request.params).await,
        _ => Err(McpError::new(METHOD_NOT_FOUND, "method not found")),
    }
}

async fn tools_call(state: &AppState, params: Value) -> Result<Value, McpError> {
    let params = parse_params::<ToolCallParams>(params)?;
    match params.name.as_str() {
        "skills.search" => skills_search(state, params.arguments).await,
        "skills.find_similar" => skills_find_similar(params.arguments).await,
        "skills.get_manifest" => skills_get_manifest(state, params.arguments).await,
        "skills.suggest_for_task" => skills_suggest_for_task(state, params.arguments).await,
        _ => Err(McpError::new(INVALID_PARAMS, "unknown tool")),
    }
}

async fn skills_search(state: &AppState, arguments: Value) -> Result<Value, McpError> {
    let args = parse_params::<SearchArgs>(arguments)?;
    let query = non_empty(args.query, "query")?;
    let limit = bounded_limit(args.limit)?;
    let index = filtered_search_index(
        state,
        SearchParams {
            query: Some(query),
            namespace: None,
            limit: Some(limit),
        },
    )
    .await
    .map_err(|_| McpError::new(INTERNAL_ERROR, "skill search failed"))?;
    Ok(tool_json(json!({"skills": index.skills, "warnings": []})))
}

async fn skills_find_similar(arguments: Value) -> Result<Value, McpError> {
    let args = parse_params::<FindSimilarArgs>(arguments)?;
    let _description = non_empty(args.description, "description")?;
    let _limit = bounded_limit(args.limit)?;
    Ok(tool_error_json(json!({
        "error": "semantic search is not configured"
    })))
}

async fn skills_get_manifest(state: &AppState, arguments: Value) -> Result<Value, McpError> {
    let args = parse_params::<GetManifestArgs>(arguments)?;
    let name = non_empty(args.name, "name")?;
    validate_skill_name(&name).map_err(|_| McpError::new(INVALID_PARAMS, "invalid skill name"))?;

    let version = match args.version {
        Some(version) => {
            let version = non_empty(version, "version")?;
            validate_version(&version)
                .map_err(|_| McpError::new(INVALID_PARAMS, "invalid version"))?;
            Some(version)
        }
        None => None,
    };

    match manifest_for_state(state, &name, version.as_deref()).await {
        Ok(manifest) => Ok(tool_json(json!({"manifest": manifest}))),
        Err(error) if error.status == StatusCode::NOT_FOUND => Ok(tool_error_json(json!({
            "error": "skill manifest was not found"
        }))),
        Err(_) => Err(McpError::new(
            INTERNAL_ERROR,
            "skill manifest lookup failed",
        )),
    }
}

async fn skills_suggest_for_task(state: &AppState, arguments: Value) -> Result<Value, McpError> {
    let args = parse_params::<SuggestForTaskArgs>(arguments)?;
    let task_description = non_empty(args.task_description, "task_description")?;
    let limit = bounded_limit(args.limit)?;
    let index = filtered_search_index(
        state,
        SearchParams {
            query: Some(task_description),
            namespace: None,
            limit: Some(limit),
        },
    )
    .await
    .map_err(|_| McpError::new(INTERNAL_ERROR, "skill suggestion failed"))?;
    Ok(tool_json(json!({
        "skills": index.skills,
        "warnings": ["semantic search is not configured; used lexical fallback"]
    })))
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {"tools": {}},
        "serverInfo": {
            "name": "agentenv-skills-hub",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn tools_list_result() -> Value {
    json!({
        "tools": [
            tool_definition("skills.search", "Search visible skills by query", json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_LIMIT}
                },
                "additionalProperties": false
            })),
            tool_definition("skills.find_similar", "Find skills similar to a description", json!({
                "type": "object",
                "required": ["description"],
                "properties": {
                    "description": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_LIMIT}
                },
                "additionalProperties": false
            })),
            tool_definition("skills.get_manifest", "Fetch a skill manifest by name and optional version", json!({
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": {"type": "string"},
                    "version": {"type": "string"}
                },
                "additionalProperties": false
            })),
            tool_definition("skills.suggest_for_task", "Suggest skills for a task description", json!({
                "type": "object",
                "required": ["task_description"],
                "properties": {
                    "task_description": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_LIMIT}
                },
                "additionalProperties": false
            }))
        ]
    })
}

fn tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn tool_json(payload: Value) -> Value {
    json!({
        "content": [{"type": "text", "text": payload.to_string()}],
        "isError": false
    })
}

fn tool_error_json(payload: Value) -> Value {
    json!({
        "content": [{"type": "text", "text": payload.to_string()}],
        "isError": true
    })
}

fn parse_params<T>(value: Value) -> Result<T, McpError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value).map_err(|_| McpError::new(INVALID_PARAMS, "invalid params"))
}

fn non_empty(value: String, field: &str) -> Result<String, McpError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(McpError::new(
            INVALID_PARAMS,
            format!("`{field}` must not be empty"),
        ));
    }
    Ok(value)
}

fn bounded_limit(limit: Option<usize>) -> Result<usize, McpError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 {
        return Err(McpError::new(
            INVALID_PARAMS,
            "`limit` must be greater than zero",
        ));
    }
    Ok(limit.min(MAX_LIMIT))
}

fn success_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION,
        id,
        result: Some(result),
        error: None,
    }
}

fn error_response(id: Value, code: i64, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION,
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
        }),
    }
}

#[derive(Debug)]
struct McpError {
    code: i64,
    message: String,
}

impl McpError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
