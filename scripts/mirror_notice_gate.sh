#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

NOTICE_DATE=""
NOTICE_SOURCE="head_commit"
WORKSPACE_DIR="${REPO_ROOT}"
STRICT_GATE_OUTPUT=""
JSON_OUTPUT=""
RUN_STRICT_GATE=true

usage() {
  cat <<'EOF'
Usage:
  scripts/mirror_notice_gate.sh [options]

Options:
  --notice-date <YYYY-MM-DD>   Notice anchor date (default: latest workspace commit date)
  --workspace-dir <path>       Workspace dir for strict gate (default: repo root)
  --strict-gate-output <path>  Output path for strict release-pack report JSON
  --json-output <path>         Write gate decision JSON to file
  --skip-strict-gate           Skip strict release-pack execution even if eligible
  -h, --help                   Show help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --notice-date)
      NOTICE_DATE="${2:-}"
      shift 2
      ;;
    --workspace-dir)
      WORKSPACE_DIR="${2:-}"
      shift 2
      ;;
    --strict-gate-output)
      STRICT_GATE_OUTPUT="${2:-}"
      shift 2
      ;;
    --json-output)
      JSON_OUTPUT="${2:-}"
      shift 2
      ;;
    --skip-strict-gate)
      RUN_STRICT_GATE=false
      shift
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

if [[ ! -d "${WORKSPACE_DIR}" ]]; then
  echo "workspace directory not found: ${WORKSPACE_DIR}" >&2
  exit 1
fi

WORKSPACE_DIR="$(cd "${WORKSPACE_DIR}" && pwd)"

if [[ -n "${NOTICE_DATE}" && ! "${NOTICE_DATE}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "notice-date must use YYYY-MM-DD: ${NOTICE_DATE}" >&2
  exit 1
fi

if [[ -z "${NOTICE_DATE}" ]]; then
  if ! git -C "${WORKSPACE_DIR}" rev-parse HEAD >/dev/null 2>&1; then
    echo "workspace directory is not a git repository with a HEAD commit: ${WORKSPACE_DIR}" >&2
    exit 1
  fi
  NOTICE_DATE="$(git -C "${WORKSPACE_DIR}" log -1 --format=%cs HEAD)"
  NOTICE_EPOCH="$(git -C "${WORKSPACE_DIR}" log -1 --format=%ct HEAD)"
else
  if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 is required when --notice-date is provided" >&2
    exit 1
  fi
  NOTICE_SOURCE="explicit_date"
  NOTICE_EPOCH="$(
    python3 - "${NOTICE_DATE}" <<'PY'
from datetime import datetime, timezone
import sys

dt = datetime.strptime(sys.argv[1], "%Y-%m-%d").replace(tzinfo=timezone.utc)
print(int(dt.timestamp()))
PY
  )"
fi

POST_NOTICE_TAG=""
POST_NOTICE_TAG_DATE=""
POST_NOTICE_TAG_EPOCH=""
while IFS= read -r row; do
  tag_name="${row%%|*}"
  remainder="${row#*|}"
  tag_date="${remainder%%|*}"
  tag_epoch="${remainder##*|}"
  if (( tag_epoch >= NOTICE_EPOCH )); then
    POST_NOTICE_TAG="${tag_name}"
    POST_NOTICE_TAG_DATE="${tag_date}"
    POST_NOTICE_TAG_EPOCH="${tag_epoch}"
    break
  fi
done < <(
  git -C "${WORKSPACE_DIR}" for-each-ref \
    --sort=-creatordate \
    --format='%(refname:short)|%(creatordate:short)|%(creatordate:unix)' refs/tags
)

strict_executed=false
strict_passed=false
strict_reason="not_applicable"
strict_report_path=""

if [[ -n "${POST_NOTICE_TAG}" ]]; then
  strict_reason="skipped_by_flag"
  if [[ "${RUN_STRICT_GATE}" == true ]]; then
    strict_executed=true
    strict_reason="executed"
    if [[ -z "${STRICT_GATE_OUTPUT}" ]]; then
      strict_report_path="${REPO_ROOT}/logs/quality/release_pack_strict_notice.json"
    else
      strict_report_path="${STRICT_GATE_OUTPUT}"
    fi
    bash "${SCRIPT_DIR}/release_pack_strict_gate.sh" \
      --workspace-dir "${WORKSPACE_DIR}" \
      --output "${strict_report_path}" >/dev/null
    if jq -e '.passed == true and .unresolved_blockers == 0' "${strict_report_path}" >/dev/null; then
      strict_passed=true
    else
      strict_passed=false
      strict_reason="failed"
    fi
  fi
fi

if [[ -z "${POST_NOTICE_TAG}" ]]; then
  status="blocked"
  reason="post_notice_tag_missing"
elif [[ "${RUN_STRICT_GATE}" != true ]]; then
  status="blocked"
  reason="strict_gate_skipped"
elif [[ "${strict_passed}" == true ]]; then
  status="ready"
  reason="post_notice_tag_and_strict_gate_passed"
else
  status="blocked"
  reason="strict_gate_failed"
fi

result_json="$(
  jq -n \
    --arg status "${status}" \
    --arg reason "${reason}" \
    --arg notice_date "${NOTICE_DATE}" \
    --arg notice_source "${NOTICE_SOURCE}" \
    --arg post_notice_tag "${POST_NOTICE_TAG}" \
    --arg post_notice_tag_date "${POST_NOTICE_TAG_DATE}" \
    --arg post_notice_tag_epoch "${POST_NOTICE_TAG_EPOCH}" \
    --argjson strict_executed "${strict_executed}" \
    --argjson strict_passed "${strict_passed}" \
    --arg strict_reason "${strict_reason}" \
    --arg strict_report_path "${strict_report_path}" \
    '{
      status: $status,
      reason: $reason,
      notice_date: $notice_date,
      notice_source: $notice_source,
      post_notice_tag: (if $post_notice_tag == "" then null else $post_notice_tag end),
      post_notice_tag_date: (if $post_notice_tag_date == "" then null else $post_notice_tag_date end),
      post_notice_tag_epoch: (if $post_notice_tag_epoch == "" then null else ($post_notice_tag_epoch | tonumber) end),
      strict_gate: {
        executed: $strict_executed,
        passed: $strict_passed,
        reason: $strict_reason,
        report_path: (if $strict_report_path == "" then null else $strict_report_path end)
      }
    }'
)"

if [[ -n "${JSON_OUTPUT}" ]]; then
  mkdir -p "$(dirname "${JSON_OUTPUT}")"
  printf '%s\n' "${result_json}" >"${JSON_OUTPUT}"
fi

printf '%s\n' "${result_json}"
