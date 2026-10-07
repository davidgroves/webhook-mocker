#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=fix-volumes.sh
source "${script_dir}/fix-volumes.sh"

echo "==> Fetching Rust dependencies"
cargo fetch

echo "==> Building debug binary (warm caches / verify toolchain)"
cargo build

echo "==> Installing Playwright e2e dependencies"
cd e2e
if [[ -f package-lock.json ]]; then
  npm ci
else
  npm install
fi

# OS libs are preinstalled in the image; still install browsers for this user.
npx playwright install chromium

echo "==> Devcontainer ready"
echo "    cargo run                 # UI on :5080"
echo "    cd e2e && npm test        # Playwright (starts server on :5099)"
