#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TODAY="$(date -u +%F)"

GATE_DOC="${ROOT_DIR}/docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md"
REQUEST_DOC="${ROOT_DIR}/docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md"
REPORT_PATH="${ROOT_DIR}/docs/RELEASE_SIGNOFF_STATUS_${TODAY}.md"

usage() {
  cat <<'EOF'
Usage:
  scripts/release_signoff_status.sh [--gate-doc <path>] [--request-doc <path>] [--report-path <path>]

Exit codes:
  0 -> READY (final release decision is DONE)
  2 -> BLOCKED (final release decision is still pending)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --gate-doc)
      GATE_DOC="${2:-}"
      shift 2
      ;;
    --request-doc)
      REQUEST_DOC="${2:-}"
      shift 2
      ;;
    --report-path)
      REPORT_PATH="${2:-}"
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

if [[ ! -f "${GATE_DOC}" ]]; then
  echo "gate document not found: ${GATE_DOC}" >&2
  exit 1
fi
if [[ ! -f "${REQUEST_DOC}" ]]; then
  echo "request document not found: ${REQUEST_DOC}" >&2
  exit 1
fi

extract_status() {
  local label="$1"
  local line status
  line="$(grep -F -- "- ${label}:" "${GATE_DOC}" | head -n 1 || true)"
  if [[ -z "${line}" ]]; then
    printf 'MISSING'
    return 0
  fi
  status="$(printf '%s' "${line}" | sed -E 's/^.*`([^`]+)`.*/\1/')"
  if [[ "${status}" == "${line}" ]]; then
    status="$(printf '%s' "${line}" | sed -E "s/^- ${label}:[[:space:]]*//")"
  fi
  printf '%s' "${status}"
}

is_done() {
  [[ "$1" == DONE* ]]
}

release_decision_status="$(extract_status "Final Release Decision")"

overall="READY"
pending_roles=()
if ! is_done "${release_decision_status}"; then
  overall="BLOCKED"
  pending_roles+=("Final Release Decision")
fi

generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
mkdir -p "$(dirname "${REPORT_PATH}")"

{
  echo "# Release Signoff Status"
  echo
  echo "Generated At (UTC): ${generated_at}"
  echo "Gate Doc: \`${GATE_DOC}\`"
  echo "Request Doc: \`${REQUEST_DOC}\`"
  echo
  echo "## Current Status"
  echo
  echo "- Overall: \`${overall}\`"
  echo "- Final Release Decision: \`${release_decision_status}\`"
  echo
  echo "## Pending Roles"
  echo
  if [[ "${#pending_roles[@]}" -eq 0 ]]; then
    echo "- none"
  else
    for role in "${pending_roles[@]}"; do
      echo "- ${role}"
    done
  fi
  echo
  echo "## Deterministic Re-check"
  echo
  echo "- Command: \`scripts/release_signoff_status.sh --report-path ${REPORT_PATH}\`"
  echo "- READY condition: \`Final Release Decision\` starts with \`DONE\` in the gate document signoff section."
} >"${REPORT_PATH}"

if [[ "${overall}" == "READY" ]]; then
  exit 0
fi
exit 2
