#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GATE_DOC=""
REQUEST_DOC=""
STATUS_DIR="${ROOT_DIR}/logs/release"
STATUS_DOC=""

decision=""
name=""
notes=""

usage() {
  cat <<'EOF'
Usage:
  scripts/record_release_signoff.sh \
    --decision <GO|NO-GO> --name <name> [--notes text] [--gate-doc <path>] [--request-doc <path>] [--status-doc <path>] [--status-dir <path>]

Defaults:
  - gate doc: docs/FEATURE_COMPLETENESS_UAT_GATE.md (fallback: latest docs/FEATURE_COMPLETENESS_UAT_GATE_*.md)
  - request doc: docs/RELEASE_SIGNOFF_REQUEST.md (fallback: latest docs/RELEASE_SIGNOFF_REQUEST_*.md)
  - status doc: <status-dir>/RELEASE_SIGNOFF_STATUS.md
  - status-dir: logs/release

Applies release decision to selected gate/request docs and refreshes status report.
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

while [[ $# -gt 0 ]]; do
  case "$1" in
    --decision) decision="${2:-}"; shift 2 ;;
    --name) name="${2:-}"; shift 2 ;;
    --notes) notes="${2:-}"; shift 2 ;;
    --gate-doc) GATE_DOC="${2:-}"; shift 2 ;;
    --request-doc) REQUEST_DOC="${2:-}"; shift 2 ;;
    --status-doc) STATUS_DOC="${2:-}"; shift 2 ;;
    --status-dir) STATUS_DIR="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
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
if [[ -z "${STATUS_DOC}" ]]; then
  STATUS_DOC="${STATUS_DIR}/RELEASE_SIGNOFF_STATUS.md"
fi

require_non_empty() {
  local value="$1"
  local label="$2"
  if [[ -z "${value}" ]]; then
    echo "missing required value: ${label}" >&2
    exit 1
  fi
}

require_non_empty "${decision}" "--decision"
require_non_empty "${name}" "--name"

if [[ "${decision}" != "GO" && "${decision}" != "NO-GO" ]]; then
  echo "invalid --decision: ${decision} (expected GO|NO-GO)" >&2
  exit 1
fi

if [[ ! -f "${GATE_DOC}" ]]; then
  echo "missing gate doc: ${GATE_DOC}" >&2
  exit 1
fi
if [[ ! -f "${REQUEST_DOC}" ]]; then
  echo "missing request doc: ${REQUEST_DOC}" >&2
  exit 1
fi

tmp_gate="$(mktemp)"
awk \
  -v decision_line="- Final Release Decision: \`DONE (${name}, ${decision})\`" \
  '
  index($0, "- Final Release Decision:") == 1 { print decision_line; next }
  { print }
  ' "${GATE_DOC}" > "${tmp_gate}"
mv "${tmp_gate}" "${GATE_DOC}"

tmp_request="$(mktemp)"
awk \
  -v decision_table="| Release Owner | Final release decision (\`GO\` or \`NO-GO\`) | DONE (${name}, ${decision}) |" \
  -v decision_line="- Decision: \`${decision}\`" \
  -v name_line="- Name: ${name}" \
  -v notes_line="- Notes: ${notes}" \
  '
  BEGIN { section = "" }
  index($0, "| Release Owner |") == 1 { print decision_table; next }
  index($0, "### Final Release Decision") == 1 { section = "decision"; print; next }
  section == "decision" && index($0, "- Decision:") == 1 { print decision_line; next }
  section == "decision" && index($0, "- Name:") == 1 { print name_line; next }
  section == "decision" && index($0, "- Notes:") == 1 { print notes_line; section = ""; next }
  { print }
  ' "${REQUEST_DOC}" > "${tmp_request}"
mv "${tmp_request}" "${REQUEST_DOC}"

"${ROOT_DIR}/scripts/release_signoff_status.sh" \
  --gate-doc "${GATE_DOC}" \
  --request-doc "${REQUEST_DOC}" \
  --report-path "${STATUS_DOC}" >/dev/null || true
echo "updated signoff docs:"
echo "- ${GATE_DOC}"
echo "- ${REQUEST_DOC}"
echo "- ${STATUS_DOC}"
