# Operations

## Solo Dev

```bash
docker compose up postgres minio
HUB_ALLOW_UNSIGNED=true HUB_DATABASE_URL=postgres://agentenv:agentenv@127.0.0.1:7778/agentenv_skills cargo run -p hub-api
```

## Team

```bash
docker compose up --build
```

Team deployments require signed publishes and bearer tokens.

## Community

Use external Postgres, OCI or S3-compatible artifact storage, signed publishes, public readable namespaces, `/metrics`, and the webhook retry worker.
