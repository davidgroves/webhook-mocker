# webhook-mocker

Dev-only Slack and Microsoft Teams incoming-webhook mocker with a live virtual-channel UI.

Ship it as a single Rust binary or a distroless container. Every request is captured with full debug data (headers, raw body, parsed JSON, validation warnings, exact response).

**Do not expose this on the public internet.** Secrets are stored unredacted.

## Quick start

```bash
cargo run
# UI: http://127.0.0.1:5080/
```

Point your app at a webhook URL on that host:

| Provider | URL shape | Success response |
|---|---|---|
| Slack Incoming Webhooks | `POST /slack/services/{T}/{B}/{token}` | `200` body `ok` |
| Teams Workflows | `POST /teams/workflows/{id}` | `202 Accepted` |
| Teams legacy O365 connector | `POST /teams/webhookb2/{id}` | `200` body `1` |
| Generic | `ANY /hooks/{id}` | `200` `{"ok":true}` |

Channels are **auto-created on first hit** by default. You can also create them in the UI or via the API.

```bash
# Preset channels at startup
cargo run -- --channel slack:alerts --channel teams:ops

# Container (binds 0.0.0.0:5080)
docker build -t webhook-mocker .
docker run --rm -p 5080:5080 -e WM_PUBLIC_URL=http://localhost:5080 webhook-mocker
```

## Configuration

| Flag / env | Default | Description |
|---|---|---|
| `--listen` / `WM_LISTEN` | `127.0.0.1:5080` | Bind address |
| `--public-url` / `WM_PUBLIC_URL` | derived from listen | Base URL printed in the UI |
| `--channel KIND:NAME` | — | Create named channel at startup (`slack`, `teams`, `teams-legacy`, `generic`) |
| `--strict` / `WM_STRICT` | `false` | Turn validation warnings into hard errors |
| `--auto-create` / `WM_AUTO_CREATE` | `true` | Auto-create channels on first hit |
| `--max-messages` / `WM_MAX_MESSAGES` | `500` | Ring buffer size per channel |
| `--max-body` / `WM_MAX_BODY` | `1048576` | Max request body bytes |
| `--snapshot` / `WM_SNAPSHOT` | — | Optional JSON snapshot path |
| `--json-logs` / `WM_JSON_LOGS` | `false` | JSON logging |

Subcommand: `webhook-mocker healthcheck` — probes `/healthz` on `--listen` (used by the distroless image).

Browsers block port `5060` (SIP). Default is `5080`. If you bind a browser-blocked port, the process warns that the UI will be unreachable.

## HTTP API (for tests / CI)

Interactive docs (OpenAPI 3 via [utoipa](https://github.com/juhaku/utoipa)):

- Swagger UI: http://127.0.0.1:5080/swagger-ui/
- Spec JSON: http://127.0.0.1:5080/api-docs/openapi.json

```bash
# List channels
curl -s localhost:5080/api/channels | jq

# Create a channel
curl -s -X POST localhost:5080/api/channels \
  -H 'content-type: application/json' \
  -d '{"kind":"slack","name":"alerts"}' | jq

# Messages / clear / wait
curl -s localhost:5080/api/channels/$ID/messages | jq
curl -s -X DELETE localhost:5080/api/channels/$ID/messages
curl -s "localhost:5080/api/channels/$ID/wait?count=1&timeout=5s" | jq

# Fault injection
curl -s -X POST localhost:5080/api/channels/$ID/faults \
  -H 'content-type: application/json' \
  -d '{"force_429":true,"retry_after_secs":2}'

# Live SSE stream
curl -N localhost:5080/api/events
```

## Example payloads

```bash
# Slack
curl -X POST http://127.0.0.1:5080/slack/services/T0/B0/dev \
  -H 'content-type: application/json' \
  -d '{"text":"Hello from Slack"}'

# Teams Workflows
curl -X POST http://127.0.0.1:5080/teams/workflows/dev \
  -H 'content-type: application/json' \
  -d @tests/fixtures/teams_adaptive.json
```

Then open the UI and click the virtual channel.

## Development

```bash
cargo test
cargo clippy --all-targets
cargo fmt
```

### Dev Container

Open the repo in VS Code / Cursor and run **Dev Containers: Reopen in Container** (or clone in a codespace-style remote). The container includes:

- Rust (with `rustfmt`, `clippy`, and the `x86_64-unknown-linux-musl` target)
- Node 22 + Playwright Chromium deps
- Docker-in-Docker (so `docker build -t webhook-mocker .` works inside the container)

On create it fetches crates, builds once, and installs `e2e` npm deps + Chromium. Ports `5080` (app) and `5099` (Playwright) are forwarded. Inside the container `WM_LISTEN` defaults to `0.0.0.0:5080` so the forwarded UI is reachable from the host.

```bash
cargo run                          # http://127.0.0.1:5080/
cd e2e && npm test                 # Playwright against :5099
docker build -t webhook-mocker .   # distroless image
```

### End-to-end tests (Playwright)

```bash
cd e2e
npm install
npx playwright install chromium
npm test
```

Playwright starts the Rust binary on port `5099` (override with `WM_E2E_PORT`), drives the UI, and posts real Slack/Teams fixtures.

## License

MIT
