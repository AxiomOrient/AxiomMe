#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GATE_DOC=""
REQUEST_DOC=""
REPORT_DIR="${ROOT_DIR}/logs/release"
REPORT_PATH=""

usage() {
  cat <<'EOF'
Usage:
  scripts/release_signoff_status.sh [--gate-doc <path>] [--request-doc <path>] [--report-path <path>] [--report-dir <path>]

Defaults:
  - gate doc: docs/FEATURE_COMPLETENESS_UAT_GATE.md (fallback: latest docs/FEATURE_COMPLETENESS_UAT_GATE_*.md)
  - request doc: docs/RELEASE_SIGNOFF_REQUEST.md (fallback: latest docs/RELEASE_SIGNOFF_REQUEST_*.md)
  - report path: <report-dir>/RELEASE_SIGNOFF_STATUS.md
  - report-dir: logs/release

Exit codes:
  0 -> READY (final release decision is DONE)
  2 -> BLOCKED (final release decision is still pending)
EOF
}

resolve_latest_doc() {
  local pattern="$1"
  local label="$2"
  local matches=()
  local latest=""
  shopt -s nullglob
  matches=(${pattern})
  shopt -u nullglob
  if [[ "${#matches[@]}" -eq 0 ]]; then
    echo "${label} document not found for pattern: ${pattern}" >&2
    exit 1
  fi
  latest="$(printf '%s\n' "${matches[@]}" | LC_ALL=C sort | tail -n 1)"
  printf '%s' "${latest}"
}

display_path() {
  local path="$1"
  if [[ "${path}" == "${ROOT_DIR}"* ]]; then
    printf '%s' "${path#${ROOT_DIR}/}"
  else
    printf '%s' "${path}"
  fi
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
    --report-dir)
      REPORT_DIR="${2:-}"
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

if [[ -z "${GATE_DOC}" ]]; then
  if [[ -f "${ROOT_DIR}/docs/FEATURE_COMPLETENESS_UAT_GATE.md" ]]; then
    GATE_DOC="${ROOT_DIR}/docs/FEATURE_COMPLETENESS_UAT_GATE.md"
  else
    GATE_DOC="$(resolve_latest_doc "${ROOT_DIR}/docs/FEATURE_COMPLETENESS_UAT_GATE_*.md" "gate")"
  fi
fi
if [[ -z "${REQUEST_DOC}" ]]; then
  if [[ -f "${ROOT_DIR}/docs/RELEASE_SIGNOFF_REQUEST.md" ]]; then
    REQUEST_DOC="${ROOT_DIR}/docs/RELEASE_SIGNOFF_REQUEST.md"
  else
    REQUEST_DOC="$(resolve_latest_doc "${ROOT_DIR}/docs/RELEASE_SIGNOFF_REQUEST_*.md" "request")"
  fi
fi
if [[ -z "${REPORT_PATH}" ]]; then
  REPORT_PATH="${REPORT_DIR}/RELEASE_SIGNOFF_STATUS.md"
fi

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

mkdir -p "$(dirname "${REPORT_PATH}")"
gate_doc_display="$(display_path "${GATE_DOC}")"
request_doc_display="$(display_path "${REQUEST_DOC}")"
report_path_display="$(display_path "${REPORT_PATH}")"

{
  echo "# Release Signoff Status"
  echo
  echo "Gate Doc: \`${gate_doc_display}\`"
  echo "Request Doc: \`${request_doc_display}\`"
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
  echo "- Command: \`scripts/release_signoff_status.sh --gate-doc ${gate_doc_display} --request-doc ${request_doc_display} --report-path ${report_path_display}\`"
  echo "- READY condition: \`Final Release Decision\` starts with \`DONE\` in the gate document signoff section."
} >"${REPORT_PATH}"

if [[ "${overall}" == "READY" ]]; then
  exit 0
fi
exit 2
