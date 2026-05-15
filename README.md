# agentenv-skills-hub

Reference implementation of a federated skill registry for agentenv-compatible skills.

The hub is a separate service from `agentenv` core. Existing `agentenv` clients consume it through the HTTP registry endpoints:

- `GET /index.json`
- `GET /skills/{name}/{version}.tar.zst`
- `GET /skills/{name}/{version}.tar.zst.sig`

The hub also exposes a read-only MCP endpoint for runtime agent discovery:

- `POST /mcp`

The MCP endpoint provides:

- `skills.search`
- `skills.find_similar`
- `skills.get_manifest`
- `skills.suggest_for_task`

## Quickstart

```bash
docker compose up --build
curl http://127.0.0.1:7777/.well-known/agent-skills
curl http://127.0.0.1:7777/index.json
```

## Compatibility

Existing `agentenv` clients can use this hub as an HTTP registry:

```yaml
skills:
  registries:
    - name: community
      type: http
      url: http://127.0.0.1:7777
```
