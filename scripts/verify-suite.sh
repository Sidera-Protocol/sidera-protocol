#!/usr/bin/env bash
# Run every check required before a Sidera release: the registry contract,
# the SDK and the dashboard, all from this repository.
#
#   npm ci                       # once, to install the SDK and dashboard
#   bash scripts/verify-suite.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

run_step() {
  echo
  echo "== $* =="
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

require_command cargo
require_command npm
[ -d "$ROOT/node_modules" ] || fail "run 'npm ci' in $ROOT first"

run_step "Contract formatting"
(cd "$ROOT" && cargo fmt --all -- --check)

run_step "Contract lint"
(cd "$ROOT" && cargo clippy --workspace --all-targets --locked -- -D warnings)

run_step "Contract tests"
(cd "$ROOT" && cargo test --workspace --all-targets --locked)

run_step "Contract documentation"
(cd "$ROOT" && RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked --document-private-items)

run_step "Contract release build"
(cd "$ROOT" && cargo build --locked --release --target wasm32v1-none -p sidera-registry)

run_step "SDK typecheck"
(cd "$ROOT" && npm run lint -w @sidera-protocol/sdk)
run_step "SDK tests"
(cd "$ROOT" && npm test -w @sidera-protocol/sdk)
run_step "SDK build"
(cd "$ROOT" && npm run build -w @sidera-protocol/sdk)

run_step "Dashboard typecheck"
(cd "$ROOT" && npm run lint -w @sidera-protocol/app)
run_step "Dashboard build"
(cd "$ROOT" && npm run build -w @sidera-protocol/app)

echo
echo "Sidera verification suite passed."
