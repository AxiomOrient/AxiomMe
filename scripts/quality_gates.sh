#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

quality_report_dir="${AXIOMME_QUALITY_REPORT_DIR:-logs/quality}"
notice_gate_json="${AXIOMME_QUALITY_NOTICE_GATE_JSON:-${quality_report_dir}/mirror_notice_gate.json}"
notice_router_json="${AXIOMME_QUALITY_NOTICE_ROUTER_JSON:-${quality_report_dir}/mirror_notice_router.json}"
enforce_notice_gate="${AXIOMME_QUALITY_ENFORCE_MIRROR_NOTICE:-off}"

mkdir -p "$(dirname "$notice_gate_json")" "$(dirname "$notice_router_json")"

echo "[quality] prohibited tokens"
bash scripts/check_prohibited_tokens.sh

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if ! command -v cargo-audit >/dev/null 2>&1; then
  echo "cargo-audit is required (install: cargo install --locked cargo-audit)" >&2
  exit 1
fi

echo "[quality] dependency audit"
cargo audit --deny unsound --deny unmaintained --deny yanked

echo "[quality] formatting"
cargo fmt --all -- --check

echo "[quality] clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "[quality] om bridge invariants"
cargo test -p axiomme-core om_reflection_apply_uses_generation_cas_and_event_idempotency --quiet

echo "[quality] workspace tests"
cargo test --workspace --quiet

echo "[quality] mirror notice router smoke"
bash scripts/mirror_notice_router_smoke.sh

echo "[quality] mirror one-cycle notice gate"
bash scripts/mirror_notice_gate.sh --workspace-dir "$repo_root" --json-output "$notice_gate_json" >/dev/null
notice_status="$(jq -r '.status' "$notice_gate_json")"
notice_reason="$(jq -r '.reason' "$notice_gate_json")"
echo "[quality] mirror notice status: ${notice_status} (${notice_reason})"

echo "[quality] mirror notice router"
bash scripts/mirror_notice_router.sh --gate-json "$notice_gate_json" --output "$notice_router_json" >/dev/null
router_next="$(jq -r '.selected_for_next' "$notice_router_json")"
router_type="$(jq -r '.route_type' "$notice_router_json")"
router_reason="$(jq -r '.route_reason' "$notice_router_json")"
echo "[quality] mirror notice router selected: ${router_next} (${router_type}/${router_reason})"

if [[ "$enforce_notice_gate" == "on" && "$notice_status" != "ready" ]]; then
  echo "[quality] mirror notice gate is not ready and AXIOMME_QUALITY_ENFORCE_MIRROR_NOTICE=on" >&2
  exit 1
fi

echo "[quality] all gates passed"
