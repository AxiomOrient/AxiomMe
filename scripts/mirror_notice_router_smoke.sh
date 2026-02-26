#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROUTER_SCRIPT="${SCRIPT_DIR}/mirror_notice_router.sh"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if [[ ! -x "${ROUTER_SCRIPT}" ]]; then
  echo "router script is not executable: ${ROUTER_SCRIPT}" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

assert_case() {
  local case_name="$1"
  local status="$2"
  local reason="$3"
  local post_notice_tag="$4"
  local expected_next="$5"
  local expected_type="$6"
  local expected_reason="$7"

  local gate_json="${tmp_dir}/${case_name}_gate.json"
  local router_json="${tmp_dir}/${case_name}_router.json"

  jq -n \
    --arg status "${status}" \
    --arg reason "${reason}" \
    --arg post_notice_tag "${post_notice_tag}" \
    '{
      status: $status,
      reason: $reason,
      post_notice_tag: (if $post_notice_tag == "" then null else $post_notice_tag end)
    }' >"${gate_json}"

  bash "${ROUTER_SCRIPT}" --gate-json "${gate_json}" --output "${router_json}" >/dev/null

  local next_action route_type route_reason
  next_action="$(jq -r '.selected_for_next' "${router_json}")"
  route_type="$(jq -r '.route_type' "${router_json}")"
  route_reason="$(jq -r '.route_reason' "${router_json}")"

  if [[ "${next_action}" != "${expected_next}" ]]; then
    echo "[router-smoke] ${case_name} selected_for_next mismatch: expected=${expected_next} actual=${next_action}" >&2
    exit 1
  fi
  if [[ "${route_type}" != "${expected_type}" ]]; then
    echo "[router-smoke] ${case_name} route_type mismatch: expected=${expected_type} actual=${route_type}" >&2
    exit 1
  fi
  if [[ "${route_reason}" != "${expected_reason}" ]]; then
    echo "[router-smoke] ${case_name} route_reason mismatch: expected=${expected_reason} actual=${route_reason}" >&2
    exit 1
  fi
}

assert_case "waiting_post_notice_tag" "blocked" "post_notice_tag_missing" "" "NX-011" "waiting" "waiting_for_post_notice_tag"
assert_case "strict_gate_recovery" "blocked" "strict_gate_failed" "0.1.3" "NX-010" "actionable" "strict_gate_recovery_required"
assert_case "ready_path" "ready" "post_notice_tag_and_strict_gate_passed" "0.1.3" "NX-009" "actionable" "ready_or_unknown"

echo "[router-smoke] all route expectations passed"
