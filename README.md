# webhook-mocker

Dev-only Slack and Microsoft Teams incoming-webhook mocker with a live virtual-channel UI.

Every request is captured with full debug data (headers, raw body, parsed JSON, validation warnings, exact response).

**Do not expose this on the public internet.** Secrets are stored unredacted.

Contributing or hacking on the source? See [DEVELOPMENT.md](DEVELOPMENT.md).

## Install

Download a binary from [GitHub Releases](https://github.com/davidgroves/webhook-mocker/releases), or pull the container:

| Platform | Artifact |
|---|---|
| Linux amd64 / arm64 (static musl) | `webhook-mocker-<version>-linux-amd64.tar.gz` / `…-linux-arm64.tar.gz` |
| macOS arm64 | `webhook-mocker-<version>-darwin-arm64.tar.gz` |
| Windows amd64 | `webhook-mocker-<version>-windows-amd64.zip` |
| Container (`linux/amd64`, `linux/arm64`) | `ghcr.io/davidgroves/webhook-mocker:<version>` |

```bash
# Binary (example: Linux amd64)
tar -xzf webhook-mocker-*-linux-amd64.tar.gz
chmod +x webhook-mocker
./webhook-mocker

# Container
docker pull ghcr.io/davidgroves/webhook-mocker:latest
docker run --rm -p 5080:5080 -e WM_PUBLIC_URL=http://localhost:5080 \
  ghcr.io/davidgroves/webhook-mocker:latest
```

UI: http://127.0.0.1:5080/

## Quick start

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
./webhook-mocker --channel slack:alerts --channel teams:ops
```

## Configuration

Settings load in this order (later wins): **defaults → YAML file → `WM_*` env → CLI flags**.

### YAML file

Pass a file with `--config` / `-c`, or `WM_CONFIG`. If neither is set, `./webhook-mocker.yaml` (or `.yml`) is loaded when present. See [`webhook-mocker.example.yaml`](webhook-mocker.example.yaml).

```yaml
listen: 0.0.0.0:5080
public_url: http://localhost:5080
channels:
  - slack:alerts
  - kind: teams
    name: ops
  # Stable path + fault profile (defect) applied at startup
  - kind: slack
    name: flaky
    token: flaky-token
    slack_team: T0
    slack_bot: B0
    faults:
      force_429: true
      retry_after_secs: 2
  - kind: generic
    name: slow
    token: slow-hook
    faults:
      delay_ms: 1500
      fail_percent: 25
      status: 503
strict: false
auto_create: true
max_messages: 500
max_body: 1048576
json_logs: false
```

Channel `faults` uses the same fields as `POST /api/channels/{id}/faults` (`force_429`, `retry_after_secs`, `delay_ms`, `delay_random_ms`, `status`, `fail_percent`). Optional `token` / `slack_team` / `slack_bot` pin a stable webhook URL.

```bash
./webhook-mocker --config ./webhook-mocker.yaml
./webhook-mocker -c ./webhook-mocker.yaml --listen 127.0.0.1:5080
```

### Flags and environment

| Flag / env | Default | Description |
|---|---|---|
| `--config` / `-c` / `WM_CONFIG` | `./webhook-mocker.yaml` if present | YAML config path |
| `--listen` / `WM_LISTEN` | `127.0.0.1:5080` | Bind address (container image defaults to `0.0.0.0:5080`) |
| `--public-url` / `WM_PUBLIC_URL` | derived from listen | Base URL printed in the UI |
| `--channel KIND:NAME` | — | Startup channels (replaces `channels` from the file when set) |
| `--strict` / `WM_STRICT` | `false` | Turn validation warnings into hard errors |
| `--auto-create` / `WM_AUTO_CREATE` | `true` | Auto-create channels on first hit |
| `--max-messages` / `WM_MAX_MESSAGES` | `500` | Ring buffer size per channel |
| `--max-body` / `WM_MAX_BODY` | `1048576` | Max request body bytes |
| `--snapshot` / `WM_SNAPSHOT` | — | Optional JSON snapshot path |
| `--json-logs` / `WM_JSON_LOGS` | `false` | JSON logging |

Boolean flags accept `--strict` / `--strict=false` (same for `--auto-create`, `--json-logs`).

Subcommand: `webhook-mocker healthcheck` — probes `/healthz` on the configured listen address (used by the container image).

Default listen port is `5080`. If you bind a browser-blocked port (e.g. `5060`), the process warns that the UI will be unreachable.

## HTTP API

Interactive docs:

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

# Teams Workflows (minimal Adaptive Card)
curl -X POST http://127.0.0.1:5080/teams/workflows/dev \
  -H 'content-type: application/json' \
  -d '{"type":"message","attachments":[{"contentType":"application/vnd.microsoft.card.adaptive","content":{"type":"AdaptiveCard","version":"1.4","body":[{"type":"TextBlock","text":"Hello from Teams"}]}}]}'
```

Then open the UI and click the virtual channel.

