#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GATE_DOC="${ROOT_DIR}/docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md"
REQUEST_DOC="${ROOT_DIR}/docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md"
STATUS_DOC="${ROOT_DIR}/docs/RELEASE_SIGNOFF_STATUS_$(date -u +%F).md"

decision=""
name=""
decision_date="$(date -u +%F)"
notes=""

usage() {
  cat <<'EOF'
Usage:
  scripts/record_release_signoff.sh \
    --decision <GO|NO-GO> --name <name> [--date YYYY-MM-DD] [--notes text]

Applies release decision to:
  - docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md
  - docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md
Then refreshes:
  - docs/RELEASE_SIGNOFF_STATUS_<today>.md
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --decision) decision="${2:-}"; shift 2 ;;
    --name) name="${2:-}"; shift 2 ;;
    --date) decision_date="${2:-}"; shift 2 ;;
    --notes) notes="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

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
  -v decision_line="- Final Release Decision: \`DONE (${decision_date}, ${name}, ${decision})\`" \
  '
  index($0, "- Final Release Decision:") == 1 { print decision_line; next }
  { print }
  ' "${GATE_DOC}" > "${tmp_gate}"
mv "${tmp_gate}" "${GATE_DOC}"

tmp_request="$(mktemp)"
awk \
  -v decision_table="| Release Owner | Final release decision (\`GO\` or \`NO-GO\`) | DONE (${decision_date}, ${name}, ${decision}) |" \
  -v decision_line="- Decision: \`${decision}\`" \
  -v name_line="- Name: ${name}" \
  -v date_line="- Date (YYYY-MM-DD): ${decision_date}" \
  -v notes_line="- Notes: ${notes}" \
  '
  BEGIN { section = "" }
  index($0, "| Release Owner |") == 1 { print decision_table; next }
  index($0, "### Final Release Decision") == 1 { section = "decision"; print; next }
  section == "decision" && index($0, "- Decision:") == 1 { print decision_line; next }
  section == "decision" && index($0, "- Name:") == 1 { print name_line; next }
  section == "decision" && index($0, "- Date (YYYY-MM-DD):") == 1 { print date_line; next }
  section == "decision" && index($0, "- Notes:") == 1 { print notes_line; section = ""; next }
  { print }
  ' "${REQUEST_DOC}" > "${tmp_request}"
mv "${tmp_request}" "${REQUEST_DOC}"

"${ROOT_DIR}/scripts/release_signoff_status.sh" --report-path "${STATUS_DOC}" >/dev/null || true
echo "updated signoff docs:"
echo "- ${GATE_DOC}"
echo "- ${REQUEST_DOC}"
echo "- ${STATUS_DOC}"
