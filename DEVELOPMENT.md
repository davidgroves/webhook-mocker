# Development

Onboarding notes for humans and AI agents working on **webhook-mocker**.

User-facing install, config, and HTTP shapes live in [README.md](README.md). This file is about how to change and verify the code: Axum + Tokio, Askama UI, in-memory channel store, Playwright e2e, CI/release.

**Do not expose a running instance on the public internet.** Captured secrets and bodies are stored unredacted.

## Prefer the Dev Container

The recommended environment is the Dev Container (`.devcontainer/`). In VS Code / Cursor: **Dev Containers: Reopen in Container**.

It provides:

| Piece | Details |
|---|---|
| Rust | Stable toolchain with `rustfmt`, `clippy`, and `x86_64-unknown-linux-musl` (matches the release image) |
| Node | Node 22 for the `e2e/` Playwright suite |
| Docker-in-Docker | So `docker build -t webhook-mocker .` works inside the container |
| Tooling | `gh`, rust-analyzer, Playwright and Docker extensions |

On create (`post-create.sh`): `cargo fetch`, `cargo build`, `npm ci` in `e2e/`, and Chromium via `npx playwright install chromium`.

On start (`post-start.sh`): reinstalls Playwright Chromium if the browser cache is missing after a volume-only recreate.

Named volumes persist Cargo registry/git caches and `target/` across rebuilds. Ports **5080** (app UI) and **5099** (Playwright’s test server) are forwarded. Inside the container:

- `WM_LISTEN` defaults to `0.0.0.0:5080` so the host can reach the UI through the forward
- `WM_PUBLIC_URL` defaults to `http://127.0.0.1:5080` for links shown in the UI

```bash
cargo run                          # UI: http://127.0.0.1:5080/
cd e2e && npm test                 # Playwright (starts binary on :5099)
docker build -t webhook-mocker .   # distroless image
```

Working outside the container is fine if you install a recent Rust toolchain, Node 22, and Playwright’s Chromium deps yourself.

## Repository map

Request path in short: provider route → optional fault decision → capture into `Store` → provider-shaped HTTP response → UI/API/SSE consumers.

Templates use Askama (`askama.toml`). Static assets are embedded via `rust-embed`. OpenAPI is generated with utoipa (`/swagger-ui/`, `/api-docs/openapi.json`).

## Day-to-day commands

```bash
cargo run
cargo test
cargo clippy --all-targets -- -D warnings   # same bar as CI
cargo fmt --check                           # or cargo fmt to fix

cd e2e && npm test                          # full Playwright suite
docker build -t webhook-mocker .
```

Useful while iterating:

```bash
cargo run -- --channel slack:alerts --channel teams:ops
RUST_LOG=webhook_mocker=debug,tower_http=info cargo run
```

Runtime settings are layered in `src/config.rs` via [config-rs](https://docs.rs/config): defaults → YAML (`--config` / `WM_CONFIG` / `./webhook-mocker.yaml`) → `WM_*` env → CLI. User-facing docs are in the README. Subcommand `webhook-mocker healthcheck` probes `/healthz` (used by the image `HEALTHCHECK`).

Browsers block port **5060** (SIP). Default listen port is **5080**. Binding a blocked port makes the UI unreachable from a browser even if the process is healthy.

## Playwright (`e2e/`)

End-to-end tests live in `e2e/` as their own npm package (`@playwright/test`). They exercise the real UI and post Slack/Teams payloads (including fixtures under `tests/fixtures/`).

### How the server is started

`e2e/playwright.config.ts` uses Playwright’s `webServer` to run:

```text
cargo run --quiet --manifest-path ../Cargo.toml -- --listen 127.0.0.1:${WM_E2E_PORT:-5099}
```

It waits for `/healthz`, uses a single worker (`workers: 1`), and sets `reuseExistingServer: !process.env.CI` so a local server on that port can be reused outside CI.

| Env | Role |
|---|---|
| `WM_E2E_PORT` | Listen/base URL port (default `5099`) |
| `CI` | Stricter mode: no `test.only`, one retry, GitHub reporter |

### Commands

```bash
cd e2e
npm install                    # or npm ci when lockfile is present
npx playwright install chromium
npm test                       # headless Chromium
npm run test:headed            # headed browser
npm run test:ui                # Playwright UI mode
npm run report                 # open last HTML report
```

In the Dev Container, install + Chromium are already handled by `post-create.sh`.

### Specs

| File | Focus |
|---|---|
| `e2e/tests/home.spec.ts` | Landing / shell |
| `e2e/tests/channels.spec.ts` | Channel CRUD via UI/API |
| `e2e/tests/webhooks.spec.ts` | Capture + rendered Slack/Teams payloads |

When posting a webhook then asserting on the channel page, prefer **post first, then `goto` the channel** (see helpers in `webhooks.spec.ts`). Navigating while SSE-driven full-page reloads are in flight races flakily.

On CI failure, the workflow uploads `e2e/playwright-report/` as an artifact.

## Rust tests and CI

- Unit/integration: `cargo test` (see `tests/integration.rs` — README HTTP examples — and helpers under `tests/fixtures/`).
- CI (`.github/workflows/ci.yml`):
  1. `cargo fmt --check`, `clippy -D warnings`, `cargo test`
  2. Playwright e2e (needs the previous job)
  3. On push only: `docker build`

- Release (`.github/workflows/release.yml`) on tags matching `v[0-9]+.[0-9]+.[0-9]+*`:
  1. Native binaries on matching runners (ubuntu / ubuntu-24.04-arm / macos-14 / windows) → GitHub Release (+ `SHA256SUMS`)
  2. Native Docker builds on amd64 + arm64 runners, then a multi-arch manifest → `ghcr.io/<owner>/webhook-mocker` (`latest`, semver tags)

Match the CI order locally before opening a PR. To cut a release: `git tag v0.1.0 && git push origin v0.1.0`.

## Conventions for changes

1. **README examples need tests** — Every concrete example in [README.md](README.md) (provider URL table, example payloads, HTTP API curls, config shapes) must have matching automated coverage. Prefer `tests/integration.rs` for HTTP contracts; use `src/config.rs` unit tests for YAML/`WM_*`/CLI layering; use Playwright only when the behaviour is UI-specific. If you add or change a README example, add or update the test in the same change.
2. **Provider parity** — Success status/body must stay aligned with the README table (`ok`, `202`, `1`, `{"ok":true}`, etc.). Update README + fixtures/e2e when changing shapes.
3. **Store is the source of truth** — Captures, faults, and SSE (`/api/events`) go through `store`. Keep API and UI consistent with what the store emits.
4. **Renderers are pure-ish** — Payload → structured `RenderBlock`s / HTML lives in `src/render/`. Prefer adding fixture JSON under `tests/fixtures/` and covering via Rust tests and/or Playwright.
5. **UI assets** — `templates/index.html`, `assets/app.css`, `assets/app.js`. Rebuild/restart after template or asset edits (`cargo run` picks up compile-time embeds).
6. **Security posture** — This tool intentionally keeps secrets for debugging. Do not add “phone home,” default public binds in non-container local runs beyond what README/devcontainer already document, or redaction that breaks the debug mission without an explicit product decision.
7. **gitignore** — Ignore `target/`, Playwright outputs, `.env`, and core dumps (`core`, `core.*`). Do not commit `e2e/node_modules/` or `e2e/test-results/`.

## Quick verification checklist

After a non-trivial change:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cd e2e && npm test
```

Smoke the UI once with a real fixture if you touched providers or renderers:

```bash
cargo run &
curl -X POST http://127.0.0.1:5080/slack/services/T0/B0/dev \
  -H 'content-type: application/json' \
  -d '{"text":"dev smoke"}'
# open http://127.0.0.1:5080/ and the new channel
```
