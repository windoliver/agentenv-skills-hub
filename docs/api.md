# API Reference

## Compatibility

- `GET /index.json`
- `GET /skills/{name}/{version}.tar.zst`
- `GET /skills/{name}/{version}.tar.zst.sig`

`/index.json` returns:

```json
{
  "skills": [
    {
      "name": "code-review",
      "version": "1.2.0",
      "description": "Review code changes",
      "registry": "community",
      "digest": "sha256:...",
      "signature_ed25519": "...",
      "public_key_ed25519": "..."
    }
  ]
}
```

## Hub API

- `GET /.well-known/agent-skills`
- `GET /api/v1/skills?query=&namespace=&limit=`
- `POST /api/v1/skills/{namespace}/{name}/versions`
- `POST /api/v1/skills/{namespace}/{name}/versions/{version}/yank`
- `POST /api/v1/skills/{namespace}/{name}/versions/{version}/unyank`
- `GET /api/v1/search?q=&namespace=&semantic=true&limit=`
- `POST /api/v1/search/similar`
- `GET /api/v1/webhooks`
- `POST /api/v1/webhooks`
- `DELETE /api/v1/webhooks/{id}`
- `GET /api/v1/healthz`
- `GET /api/v1/readyz`
- `GET /metrics`
