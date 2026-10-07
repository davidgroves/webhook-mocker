#!/usr/bin/env bash
# Named Docker volumes mount as root; make them writable for the vscode user.
set -euo pipefail

cargo_home="${CARGO_HOME:-/usr/local/cargo}"
paths=(
  "${cargo_home}/registry"
  "${cargo_home}/git"
  "target"
)

for path in "${paths[@]}"; do
  if [[ -e "${path}" ]] && [[ ! -w "${path}" ]]; then
    echo "==> Fixing ownership of ${path}"
    sudo chown -R "$(id -u):$(id -g)" "${path}"
  fi
done
