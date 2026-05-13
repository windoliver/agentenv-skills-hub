# agentenv-skills-hub

Reference implementation of a federated skill registry for agentenv-compatible skills.

The hub is a separate service from `agentenv` core. Existing `agentenv` clients consume it through the HTTP registry endpoints:

- `GET /index.json`
- `GET /skills/{name}/{version}.tar.zst`
- `GET /skills/{name}/{version}.tar.zst.sig`
