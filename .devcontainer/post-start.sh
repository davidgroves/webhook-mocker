#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=fix-volumes.sh
source "${script_dir}/fix-volumes.sh"

# Ensure Playwright browsers exist after a volume-only recreate.
if [[ -d e2e/node_modules ]] && [[ ! -d "${HOME}/.cache/ms-playwright" ]]; then
  echo "==> Restoring Playwright Chromium"
  (cd e2e && npx playwright install chromium)
fi
