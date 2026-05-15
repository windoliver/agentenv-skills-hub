# M7-9 Hub MCP Design

- Date: 2026-05-14
- Issue: https://github.com/windoliver/agentenv/issues/35
- Repository: `windoliver/agentenv-skills-hub`
- Milestone: M7 Skills axis and registry
- Depends on:
  - https://github.com/windoliver/agentenv/issues/34
  - https://github.com/windoliver/agentenv/issues/9
- Affected crates: `hub-api`, `hub-core`, `hub-search`

## 1. Context And Goals

Issue #35 asks the skill hub to expose runtime skill discovery over MCP, so an
agent can query the skill registry through the same rail it already uses for
context. The hub already exposes HTTP registry compatibility endpoints and a
REST API. This design adds a read-only MCP endpoint to the hub itself.

The first PR targets `agentenv-skills-hub` because the hub owns the
`skills.search`, `skills.find_similar`, `skills.get_manifest`, and
`skills.suggest_for_task` tools. `agentenv` core wiring for
`skills.runtime_discovery.mcp_endpoint` remains a follow-up integration PR so
the agent-side config can point at this hub endpoint without turning core into
a proxy.

Goals:

1. Add an MCP HTTP endpoint at `/mcp`.
2. Implement MCP `initialize`, `tools/list`, and `tools/call` for the four
   issue tools.
3. Reuse existing hub state, repository, and search code so MCP and REST
   return consistent skill summaries.
4. Keep the MCP surface read-only. Publishing, yanking, and webhook management
   stay REST-only.
5. Preserve existing REST and compatibility endpoints.
6. Advertise `/mcp` from `/.well-known/agent-skills`.
7. Add contract tests for MCP behavior, malformed requests, and degraded
   semantic search behavior.

## 2. Non-Goals

1. Do not move the hub into `agentenv` core or vendor the hub into the main
   `agentenv` repository.
2. Do not add new write-capable MCP tools.
3. Do not add Python, Node, OpenSSL, or an external MCP gateway dependency.
4. Do not implement `agentenv.yaml` runtime-discovery injection in this PR.
5. Do not change the existing HTTP registry compatibility contract.
6. Do not require semantic search to be configured for lexical search and
   manifest reads to work.

## 3. MCP Endpoint

Add `POST /mcp` to `hub-api`. The endpoint accepts JSON-RPC 2.0 request
objects and returns one JSON-RPC response object per request. Batch support is
not required for the first implementation.

Supported MCP methods:

1. `initialize`
2. `tools/list`
3. `tools/call`

`initialize` returns an MCP initialize result with a hub server identity:

```json
{
  "protocolVersion": "2024-11-05",
  "capabilities": {
    "tools": {}
  },
  "serverInfo": {
    "name": "agentenv-skills-hub",
    "version": "0.1.0"
  }
}
```

`tools/list` returns four tools:

1. `skills.search`
2. `skills.find_similar`
3. `skills.get_manifest`
4. `skills.suggest_for_task`

Each tool includes a JSON Schema input shape. Tool result payloads are returned
as MCP content entries containing compact JSON text. The JSON text keeps a
stable, documented shape and avoids leaking internal database fields.

Update `/.well-known/agent-skills` to advertise the MCP endpoint:

```json
{
  "schema_version": "0.1",
  "registry": {
    "type": "agentenv-skills-hub",
    "index": "/index.json",
    "api": "/api/v1",
    "mcp": "/mcp"
  }
}
```

## 4. Tool Semantics

### `skills.search`

Input:

```json
{
  "query": "pdf parsing",
  "limit": 20
}
```

Behavior:

1. Require a non-empty `query`.
2. Default `limit` to `20`.
3. Clamp `limit` to `50`.
4. Use the same filtered lexical search path as `GET /api/v1/search`.
5. Return `[SkillSummary]`.

### `skills.find_similar`

Input:

```json
{
  "description": "Review pull requests and produce actionable comments",
  "limit": 10
}
```

Behavior:

1. Require a non-empty `description`.
2. Default `limit` to `20` and clamp it to the same maximum as search.
3. Use configured semantic search when available.
4. If semantic search is unavailable, return an MCP tool error that clearly
   says semantic search is not configured.

The existing semantic backend accepts embeddings, not raw text. This issue
therefore adds a small abstraction for text-to-similar lookup. The default
implementation returns a semantic-unavailable error. Configured semantic
backends can implement real embedding lookup behind the trait without exposing
embedding details to MCP clients.

### `skills.get_manifest`

Input:

```json
{
  "name": "code-review",
  "version": "1.2.0"
}
```

Behavior:

1. Require a valid skill `name`.
2. If `version` is supplied, validate it and return that exact version.
3. If `version` is omitted, select the highest non-yanked semver version that
   is visible to the caller.
4. Return a `SkillManifest` shape compatible with the hub model:

```json
{
  "name": "code-review",
  "version": "1.2.0",
  "description": "Review code changes",
  "entry": "SKILL.md",
  "files": ["SKILL.md"]
}
```

Current REST compatibility responses do not include the full manifest. The
implementation should add a repository/read-model method that can return
manifest metadata for a version, then have the MCP tool call that method. In
fixture mode, the tool returns the bundled fixture manifest.

### `skills.suggest_for_task`

Input:

```json
{
  "task_description": "I need to parse PDF invoices into markdown",
  "limit": 10
}
```

Behavior:

1. Require a non-empty `task_description`.
2. Use semantic search when configured.
3. Fall back to lexical search against the task description when semantic
   search is unavailable, and include a warning in the JSON result explaining
   the fallback.
4. Return `[SkillSummary]`.

This differs intentionally from `find_similar`: suggesting for a task may still
be useful with lexical matching, while finding semantic near-duplicates should
report degraded capability honestly when semantic search is absent.

## 5. Data Shapes

`SkillSummary`:

```json
{
  "name": "code-review",
  "version": "1.2.0",
  "description": "Review code changes",
  "registry": "community",
  "digest": "sha256:...",
  "signature_ed25519": "...",
  "public_key_ed25519": "..."
}
```

`SkillManifest` uses the existing `hub_core::model::SkillManifest`.

Tool result JSON:

```json
{
  "skills": [SkillSummary],
  "warnings": []
}
```

for search-like tools, and:

```json
{
  "manifest": SkillManifest
}
```

for `skills.get_manifest`.

## 6. Internal Structure

Add a small `hub-api::mcp` module:

1. JSON-RPC request/response structs.
2. MCP initialize and tool-list response builders.
3. Tool input structs with serde validation.
4. Tool dispatcher.
5. Result rendering helpers.

Keep business logic outside the HTTP handler:

1. Add `McpSkillService` or equivalent helper in `hub-api` that wraps
   `AppState`.
2. Reuse `filtered_search_index` logic by extracting it from handlers into a
   shared helper.
3. Add repository methods for manifest reads when state uses Postgres.
4. Keep fixture behavior deterministic and in memory.

Avoid introducing a new crate unless the module grows enough to justify it.
The expected first implementation is small enough to live in `hub-api`.

## 7. Security And Policy

The MCP endpoint is read-only. It must not expose publish, yank, unyank,
webhook, storage, or admin operations.

Authorization follows current hub behavior:

1. Public unauthenticated reads remain available.
2. MCP does not broaden visibility beyond the public read models already used
   by unauthenticated REST compatibility endpoints.
3. Tool errors must not include bearer tokens, artifact credentials, storage
   credentials, or raw database connection details.

Input validation rules:

1. Reject unknown JSON-RPC methods with `-32601`.
2. Reject malformed params with `-32602`.
3. Enforce positive bounded `limit` values.
4. Validate skill names and semver versions before repository lookup.
5. Return structured JSON-RPC errors rather than HTTP 500 for tool-level
   invalid input.

## 8. Error Handling

Transport-level malformed JSON returns JSON-RPC parse or invalid-request
errors. Valid JSON-RPC requests that fail tool validation return JSON-RPC
errors with stable codes and short messages.

Tool execution failures return MCP tool results with `isError: true` when the
MCP call itself is valid but the tool could not complete, such as semantic
search being unavailable for `skills.find_similar`.

Unexpected hub failures map to internal JSON-RPC errors without exposing
database, storage, or credential details.

## 9. Testing

Add `hub-api` tests for:

1. `POST /mcp` `initialize` returns MCP capabilities and server info.
2. `tools/list` lists exactly the four read-only skill tools.
3. `tools/call` `skills.search` returns fixture skill summaries.
4. `skills.get_manifest` returns exact and latest-version manifests.
5. `skills.find_similar` reports semantic-unavailable behavior when no
   semantic backend is configured.
6. `skills.suggest_for_task` falls back to lexical search with a warning when
   semantic search is unavailable.
7. Unknown methods return `-32601`.
8. Unknown tools and malformed params return stable JSON-RPC errors.
9. Limits are clamped and invalid limits are rejected.
10. Existing REST compatibility tests still pass.

Run before merge:

```sh
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## 10. Follow-Up Agentenv Integration

After the hub PR lands, add a small `agentenv` PR that:

1. Parses:

```yaml
skills:
  runtime_discovery:
    mcp_endpoint: mcp+https://skills.acme.internal/mcp
    scopes: ["search", "get_manifest"]
```

2. Validates and gates the endpoint through the existing MCP/SSRF and network
   policy paths.
3. Injects the hub MCP endpoint as an additional agent MCP server during
   `agentenv create`.
4. Persists the endpoint in env state without credential values.

This preserves the architecture decision that skills are core-managed
artifacts, while still letting running agents discover hub skills through MCP.
