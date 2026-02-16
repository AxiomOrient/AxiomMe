#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "[quality] prohibited tokens"
bash scripts/check_prohibited_tokens.sh

echo "[quality] formatting"
cargo fmt --all -- --check

echo "[quality] clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "[quality] om bridge invariants"
cargo test -p axiomme-core om_reflection_apply_uses_generation_cas_and_event_idempotency --quiet

echo "[quality] workspace tests"
cargo test --workspace --quiet

echo "[quality] all gates passed"
