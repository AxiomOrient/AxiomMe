#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

GATE_JSON="${REPO_ROOT}/docs/MIRROR_NOTICE_GATE_2026-02-24.json"
OUTPUT_JSON="${REPO_ROOT}/docs/MIRROR_NOTICE_ROUTER_2026-02-24.json"

usage() {
  cat <<'EOU'
Usage:
  scripts/mirror_notice_router.sh [options]

Options:
  --gate-json <path>     Input gate decision JSON (default: docs/MIRROR_NOTICE_GATE_2026-02-24.json)
  --output <path>        Output router decision JSON (default: docs/MIRROR_NOTICE_ROUTER_2026-02-24.json)
  -h, --help             Show help
EOU
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --gate-json)
      GATE_JSON="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT_JSON="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if [[ ! -f "${GATE_JSON}" ]]; then
  echo "gate json not found: ${GATE_JSON}" >&2
  exit 1
fi

status="$(jq -r '.status // "unknown"' "${GATE_JSON}")"
reason="$(jq -r '.reason // "unknown"' "${GATE_JSON}")"
post_notice_tag="$(jq -r '.post_notice_tag // ""' "${GATE_JSON}")"
generated_at_utc="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"

selected_for_next="NX-009"
route_type="actionable"
route_reason="ready_or_unknown"
action="Proceed with one-cycle readiness closure for actual notice-date gate."

if [[ "${status}" == "blocked" && "${reason}" == "post_notice_tag_missing" ]]; then
  selected_for_next="NX-011"
  route_type="waiting"
  route_reason="waiting_for_post_notice_tag"
  action="Keep NX-009 parked as blocked; refresh notice gate snapshot until a post-notice release tag appears."
elif [[ "${status}" == "blocked" && ( "${reason}" == "strict_gate_failed" || "${reason}" == "strict_gate_skipped" ) ]]; then
  selected_for_next="NX-010"
  route_type="actionable"
  route_reason="strict_gate_recovery_required"
  action="Run strict release gate remediation and clear unresolved blockers, then rerun notice gate."
fi

router_json="$(
  jq -n \
    --arg gate_json "${GATE_JSON}" \
    --arg generated_at_utc "${generated_at_utc}" \
    --arg gate_status "${status}" \
    --arg gate_reason "${reason}" \
    --arg post_notice_tag "${post_notice_tag}" \
    --arg route_type "${route_type}" \
    --arg route_reason "${route_reason}" \
    --arg selected_for_next "${selected_for_next}" \
    --arg action "${action}" \
    '{
      generated_at_utc: $generated_at_utc,
      gate_json: $gate_json,
      gate_status: $gate_status,
      gate_reason: $gate_reason,
      post_notice_tag: (if $post_notice_tag == "" then null else $post_notice_tag end),
      route_type: $route_type,
      route_reason: $route_reason,
      selected_for_next: $selected_for_next,
      action: $action
    }'
)"

mkdir -p "$(dirname "${OUTPUT_JSON}")"
printf '%s\n' "${router_json}" >"${OUTPUT_JSON}"
printf '%s\n' "${router_json}"
