#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  scripts/contextset_random_benchmark.sh [options]

Options:
  --dataset <path>                  Dataset root (default: \$AXIOMME_DATASET_DIR)
  --target-uri <axiom-uri>          Ingest target URI (default: axiom://resources/contextSet)
  --sample-size <n>                 Random heading sample size (default: 24)
  --seed <int>                      RNG seed for reproducible sampling (default: unix epoch seconds)
  --date <YYYY-MM-DD>               Report date label (default: today)
  --report-path <path>              Output markdown report path
  --root <path>                     Runtime root path (default: temp dir)
  --bin <path>                      CLI binary path (default: <repo>/target/debug/axiomme-cli)
  --skip-build                      Skip cargo build
  --keep-root                       Keep runtime root directory after run
  --find-limit <n>                  find limit per query (default: 5)
  --search-limit <n>                search limit per query (default: 5)
  --min-match-tokens <n>            search min match tokens (default: 2)
  --min-find-nonempty-rate <pct>    pass threshold (default: 90)
  --min-search-nonempty-rate <pct>  pass threshold (default: 80)
  --min-find-top1-rate <pct>        pass threshold (default: 65)
  --min-search-top1-rate <pct>      pass threshold (default: 65)
  --min-find-top5-rate <pct>        pass threshold (default: 50)
  --min-search-top5-rate <pct>      pass threshold (default: 45)
  -h, --help                        Show this help

Notes:
  - Search scenarios use isolated session ids per query to avoid cross-query hint contamination.
  - --min-match-tokens is applied only when heading token count is >= that threshold.
EOF
}

require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "missing required command: ${cmd}" >&2
    exit 1
  fi
}

validate_integer_ge() {
  local name="$1"
  local value="$2"
  local min="$3"
  if ! [[ "${value}" =~ ^-?[0-9]+$ ]]; then
    echo "${name} must be an integer: ${value}" >&2
    exit 1
  fi
  if (( value < min )); then
    echo "${name} must be >= ${min}: ${value}" >&2
    exit 1
  fi
}

validate_percent_threshold() {
  local name="$1"
  local value="$2"
  if ! awk -v v="${value}" 'BEGIN { exit (v >= 0 && v <= 100) ? 0 : 1 }'; then
    echo "${name} must be in range [0,100]: ${value}" >&2
    exit 1
  fi
}

pct() {
  local numerator="$1"
  local denominator="$2"
  awk -v n="${numerator}" -v d="${denominator}" 'BEGIN { if (d == 0) printf "0.00"; else printf "%.2f", (n * 100.0) / d }'
}

rate_meets_threshold() {
  local value="$1"
  local threshold="$2"
  awk -v v="${value}" -v t="${threshold}" 'BEGIN { exit (v + 0 >= t + 0) ? 0 : 1 }'
}

calc_percentile() {
  local file="$1"
  local percentile="$2"
  local count rank
  count="$(wc -l <"${file}" | tr -d ' ')"
  if [[ "${count}" -eq 0 ]]; then
    echo "0"
    return
  fi
  rank=$(( (count * percentile + 99) / 100 ))
  sort -n "${file}" | sed -n "${rank}p"
}

calc_mean() {
  local file="$1"
  awk '{sum += $1} END { if (NR == 0) print "0.00"; else printf "%.2f", sum / NR }' "${file}"
}

REPORT_DATE="$(date +%F)"
DATASET_DIR="${AXIOMME_DATASET_DIR:-}"
TARGET_URI="axiom://resources/contextSet"
SAMPLE_SIZE=24
SEED="$(date +%s)"
REPORT_PATH=""
ROOT_DIR=""
KEEP_ROOT=false
BIN="${REPO_ROOT}/target/debug/axiomme-cli"
SKIP_BUILD=false
FIND_LIMIT=5
SEARCH_LIMIT=5
MIN_MATCH_TOKENS=2
MIN_FIND_NONEMPTY_RATE=90
MIN_SEARCH_NONEMPTY_RATE=80
MIN_FIND_TOP1_RATE=65
MIN_SEARCH_TOP1_RATE=65
MIN_FIND_TOP5_RATE=50
MIN_SEARCH_TOP5_RATE=45

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dataset)
      DATASET_DIR="${2:-}"
      shift 2
      ;;
    --target-uri)
      TARGET_URI="${2:-}"
      shift 2
      ;;
    --sample-size)
      SAMPLE_SIZE="${2:-}"
      shift 2
      ;;
    --seed)
      SEED="${2:-}"
      shift 2
      ;;
    --date)
      REPORT_DATE="${2:-}"
      shift 2
      ;;
    --report-path)
      REPORT_PATH="${2:-}"
      shift 2
      ;;
    --root)
      ROOT_DIR="${2:-}"
      shift 2
      ;;
    --bin)
      BIN="${2:-}"
      shift 2
      ;;
    --skip-build)
      SKIP_BUILD=true
      shift
      ;;
    --keep-root)
      KEEP_ROOT=true
      shift
      ;;
    --find-limit)
      FIND_LIMIT="${2:-}"
      shift 2
      ;;
    --search-limit)
      SEARCH_LIMIT="${2:-}"
      shift 2
      ;;
    --min-match-tokens)
      MIN_MATCH_TOKENS="${2:-}"
      shift 2
      ;;
    --min-find-nonempty-rate)
      MIN_FIND_NONEMPTY_RATE="${2:-}"
      shift 2
      ;;
    --min-search-nonempty-rate)
      MIN_SEARCH_NONEMPTY_RATE="${2:-}"
      shift 2
      ;;
    --min-find-top1-rate)
      MIN_FIND_TOP1_RATE="${2:-}"
      shift 2
      ;;
    --min-search-top1-rate)
      MIN_SEARCH_TOP1_RATE="${2:-}"
      shift 2
      ;;
    --min-find-top5-rate)
      MIN_FIND_TOP5_RATE="${2:-}"
      shift 2
      ;;
    --min-search-top5-rate)
      MIN_SEARCH_TOP5_RATE="${2:-}"
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

require_cmd jq
require_cmd awk
require_cmd find
require_cmd sort
require_cmd sed

if [[ ! -d "${DATASET_DIR}" ]]; then
  if [[ -z "${DATASET_DIR}" ]]; then
    echo "dataset is required (--dataset <path> or AXIOMME_DATASET_DIR)" >&2
    exit 1
  fi
  echo "dataset not found: ${DATASET_DIR}" >&2
  exit 1
fi
DATASET_DIR="${DATASET_DIR%/}"

validate_integer_ge "sample-size" "${SAMPLE_SIZE}" 1
validate_integer_ge "seed" "${SEED}" 0
validate_integer_ge "find-limit" "${FIND_LIMIT}" 1
validate_integer_ge "search-limit" "${SEARCH_LIMIT}" 1
validate_integer_ge "min-match-tokens" "${MIN_MATCH_TOKENS}" 2
validate_percent_threshold "min-find-nonempty-rate" "${MIN_FIND_NONEMPTY_RATE}"
validate_percent_threshold "min-search-nonempty-rate" "${MIN_SEARCH_NONEMPTY_RATE}"
validate_percent_threshold "min-find-top1-rate" "${MIN_FIND_TOP1_RATE}"
validate_percent_threshold "min-search-top1-rate" "${MIN_SEARCH_TOP1_RATE}"
validate_percent_threshold "min-find-top5-rate" "${MIN_FIND_TOP5_RATE}"
validate_percent_threshold "min-search-top5-rate" "${MIN_SEARCH_TOP5_RATE}"

if [[ "${REPORT_PATH}" == "" ]]; then
  REPORT_PATH="${REPO_ROOT}/logs/benchmarks/contextset_random.md"
fi
mkdir -p "$(dirname "${REPORT_PATH}")"

RAW_PATH="${REPORT_PATH%.md}.tsv"

if [[ "${ROOT_DIR}" == "" ]]; then
  ROOT_DIR="$(mktemp -d /tmp/axiomme-contextset-root-XXXXXX)"
  ROOT_IS_TEMP=true
else
  mkdir -p "${ROOT_DIR}"
  ROOT_IS_TEMP=false
fi

CANDIDATES_FILE="$(mktemp /tmp/axiomme-contextset-candidates-XXXXXX.tsv)"
SHUFFLED_FILE="$(mktemp /tmp/axiomme-contextset-shuffled-XXXXXX.tsv)"
SAMPLED_FILE="$(mktemp /tmp/axiomme-contextset-sampled-XXXXXX.tsv)"
ROWS_FILE="$(mktemp /tmp/axiomme-contextset-rows-XXXXXX.tsv)"
FIND_LAT_FILE="$(mktemp /tmp/axiomme-contextset-findlat-XXXXXX.txt)"
SEARCH_LAT_FILE="$(mktemp /tmp/axiomme-contextset-searchlat-XXXXXX.txt)"
CRUD_SOURCE_DIR=""
CRUD_SOURCE_FILE=""

cleanup() {
  rm -f "${CANDIDATES_FILE}" "${SHUFFLED_FILE}" "${SAMPLED_FILE}" "${ROWS_FILE}" "${FIND_LAT_FILE}" "${SEARCH_LAT_FILE}"
  if [[ -n "${CRUD_SOURCE_DIR}" ]]; then
    rm -rf "${CRUD_SOURCE_DIR}"
  fi
  if [[ "${KEEP_ROOT}" != "true" && "${ROOT_IS_TEMP}" == "true" ]]; then
    rm -rf "${ROOT_DIR}"
  fi
}
trap cleanup EXIT

if [[ "${SKIP_BUILD}" != "true" ]]; then
  cargo build -p axiomme-cli >/dev/null
fi
if [[ ! -x "${BIN}" ]]; then
  echo "cli binary not executable: ${BIN}" >&2
  exit 1
fi

run_text() {
  "${BIN}" --root "${ROOT_DIR}" "$@"
}

run_json() {
  local out
  out="$("${BIN}" --root "${ROOT_DIR}" "$@")"
  echo "${out}" | jq -e . >/dev/null
  printf '%s' "${out}"
}

run_text init >/dev/null
add_json="$(run_json add "${DATASET_DIR}" --target "${TARGET_URI}" --wait true --markdown-only)"
ls_recursive_json="$(run_json ls "${TARGET_URI}" --recursive)"
tree_json="$(run_json tree "${TARGET_URI}")"
add_status="$(echo "${add_json}" | jq -r '.status // "ok"')"
entry_count="$(echo "${ls_recursive_json}" | jq -r 'length')"
tree_root="$(echo "${tree_json}" | jq -r '.root.uri')"

while IFS= read -r file; do
  rel_path="${file#${DATASET_DIR}/}"
  while IFS= read -r heading; do
    safe_heading="${heading//$'\t'/ }"
    printf '%s\t%s\t%s\n' "${file}" "${TARGET_URI}/${rel_path}" "${safe_heading}" >>"${CANDIDATES_FILE}"
  done < <(
    awk '
      BEGIN {
        in_fence = 0
        in_front_matter = 0
      }
      NR == 1 && $0 ~ /^---[[:space:]]*$/ {
        in_front_matter = 1
        next
      }
      in_front_matter == 1 {
        if ($0 ~ /^---[[:space:]]*$/) {
          in_front_matter = 0
        }
        next
      }
      /^[[:space:]]*(```|~~~)/ {
        in_fence = (in_fence == 0 ? 1 : 0)
        next
      }
      in_fence == 1 {
        next
      }
      /^#{1,6}[[:space:]]+/ {
        line = $0
        sub(/^#{1,6}[[:space:]]+/, "", line)
        gsub(/\r$/, "", line)
        if (length(line) > 0) {
          print line
        }
      }
    ' "${file}"
  )
done < <(find "${DATASET_DIR}" -type f -name '*.md' | sort)

candidate_total="$(wc -l <"${CANDIDATES_FILE}" | tr -d ' ')"
if [[ "${candidate_total}" -eq 0 ]]; then
  echo "no markdown heading candidates found under dataset: ${DATASET_DIR}" >&2
  exit 1
fi

awk -F'\t' -v seed="${SEED}" 'BEGIN { srand(seed) } { printf "%.12f\t%s\n", rand(), $0 }' "${CANDIDATES_FILE}" \
  | sort -n \
  | cut -f2- >"${SHUFFLED_FILE}"
head -n "${SAMPLE_SIZE}" "${SHUFFLED_FILE}" >"${SAMPLED_FILE}"

TOTAL=0
FIND_NONEMPTY=0
SEARCH_NONEMPTY=0
FIND_TOP1=0
SEARCH_TOP1=0
FIND_TOP5=0
SEARCH_TOP5=0
SEARCH_MIN_MATCH_APPLIED=0

while IFS=$'\t' read -r file expected_uri heading; do
  if [[ -z "${file}" || -z "${expected_uri}" || -z "${heading}" ]]; then
    continue
  fi
  TOTAL=$((TOTAL + 1))
  scenario_session_id="s-contextset-random-${SEED}-${TOTAL}"

  find_json="$(run_json find "${heading}" --target "${TARGET_URI}" --limit "${FIND_LIMIT}")"
  heading_token_count="$(awk '{ print NF }' <<<"${heading}")"
  search_args=(
    search
    "${heading}"
    --target "${TARGET_URI}"
    --session "${scenario_session_id}"
    --limit "${SEARCH_LIMIT}"
  )
  if [[ "${MIN_MATCH_TOKENS}" -gt 1 && "${heading_token_count}" -ge "${MIN_MATCH_TOKENS}" ]]; then
    search_args+=(--min-match-tokens "${MIN_MATCH_TOKENS}")
    SEARCH_MIN_MATCH_APPLIED=$((SEARCH_MIN_MATCH_APPLIED + 1))
  fi
  search_json="$(run_json "${search_args[@]}")"

  find_hits="$(echo "${find_json}" | jq -r '.query_results | length')"
  search_hits="$(echo "${search_json}" | jq -r '.query_results | length')"
  find_latency_ms="$(echo "${find_json}" | jq -r '.trace.metrics.latency_ms // 0')"
  search_latency_ms="$(echo "${search_json}" | jq -r '.trace.metrics.latency_ms // 0')"
  echo "${find_latency_ms}" >>"${FIND_LAT_FILE}"
  echo "${search_latency_ms}" >>"${SEARCH_LAT_FILE}"

  find_top1_hit=0
  search_top1_hit=0
  find_top5_hit=0
  search_top5_hit=0

  if [[ "${find_hits}" -gt 0 ]]; then
    FIND_NONEMPTY=$((FIND_NONEMPTY + 1))
  fi
  if [[ "${search_hits}" -gt 0 ]]; then
    SEARCH_NONEMPTY=$((SEARCH_NONEMPTY + 1))
  fi

  if [[ "$(echo "${find_json}" | jq -r '.query_results[0].uri // ""')" == "${expected_uri}" ]]; then
    find_top1_hit=1
    FIND_TOP1=$((FIND_TOP1 + 1))
  fi
  if [[ "$(echo "${search_json}" | jq -r '.query_results[0].uri // ""')" == "${expected_uri}" ]]; then
    search_top1_hit=1
    SEARCH_TOP1=$((SEARCH_TOP1 + 1))
  fi

  if echo "${find_json}" | jq -e --arg expected "${expected_uri}" '.query_results[0:5] | any(.uri == $expected)' >/dev/null; then
    find_top5_hit=1
    FIND_TOP5=$((FIND_TOP5 + 1))
  fi
  if echo "${search_json}" | jq -e --arg expected "${expected_uri}" '.query_results[0:5] | any(.uri == $expected)' >/dev/null; then
    search_top5_hit=1
    SEARCH_TOP5=$((SEARCH_TOP5 + 1))
  fi

  safe_heading="${heading//$'\t'/ }"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "${file}" \
    "${expected_uri}" \
    "${safe_heading}" \
    "${find_hits}" \
    "${search_hits}" \
    "${find_top1_hit}" \
    "${find_top5_hit}" \
    "${search_top1_hit}" \
    "${search_top5_hit}" \
    "${find_latency_ms}" \
    "${search_latency_ms}" >>"${ROWS_FILE}"
done <"${SAMPLED_FILE}"

if [[ "${TOTAL}" -eq 0 ]]; then
  echo "sample produced zero scenarios; check dataset headings and sample size" >&2
  exit 1
fi

FIND_NONEMPTY_RATE="$(pct "${FIND_NONEMPTY}" "${TOTAL}")"
SEARCH_NONEMPTY_RATE="$(pct "${SEARCH_NONEMPTY}" "${TOTAL}")"
FIND_TOP1_RATE="$(pct "${FIND_TOP1}" "${TOTAL}")"
SEARCH_TOP1_RATE="$(pct "${SEARCH_TOP1}" "${TOTAL}")"
FIND_TOP5_RATE="$(pct "${FIND_TOP5}" "${TOTAL}")"
SEARCH_TOP5_RATE="$(pct "${SEARCH_TOP5}" "${TOTAL}")"
UNIQUE_HEADING_COUNT="$(cut -f3 "${ROWS_FILE}" | sort -u | wc -l | tr -d ' ')"
AMBIGUOUS_HEADING_COUNT=$((TOTAL - UNIQUE_HEADING_COUNT))

FIND_P50_MS="$(calc_percentile "${FIND_LAT_FILE}" 50)"
FIND_P95_MS="$(calc_percentile "${FIND_LAT_FILE}" 95)"
SEARCH_P50_MS="$(calc_percentile "${SEARCH_LAT_FILE}" 50)"
SEARCH_P95_MS="$(calc_percentile "${SEARCH_LAT_FILE}" 95)"
FIND_MEAN_MS="$(calc_mean "${FIND_LAT_FILE}")"
SEARCH_MEAN_MS="$(calc_mean "${SEARCH_LAT_FILE}")"

CRUD_TOKEN_CREATE="crud-create-${SEED}"
CRUD_TOKEN_UPDATE="crud-update-${SEED}"
CRUD_URI="${TARGET_URI}/manual-crud/auto-crud-${SEED}.md"
CRUD_SOURCE_DIR="$(mktemp -d /tmp/axiomme-contextset-crud-src-XXXXXX)"
CRUD_SOURCE_FILE="${CRUD_SOURCE_DIR}/auto-crud-${SEED}.md"
printf '# Random CRUD\n\n%s\n' "${CRUD_TOKEN_CREATE}" >"${CRUD_SOURCE_FILE}"
crud_create_json="$(run_json add "${CRUD_SOURCE_FILE}" --target "${TARGET_URI}/manual-crud" --wait true --markdown-only)"
crud_load_json="$(run_json document load "${CRUD_URI}" --mode markdown)"
crud_etag="$(echo "${crud_load_json}" | jq -r '.etag')"
crud_update_json="$(run_json document save "${CRUD_URI}" --mode markdown --content $'# Random CRUD\n\n'"${CRUD_TOKEN_UPDATE}"$'\n' --expected-etag "${crud_etag}")"
crud_readback="$(run_text read "${CRUD_URI}")"
echo "${crud_readback}" | grep -Fq "${CRUD_TOKEN_UPDATE}"
run_json rm "${CRUD_URI}" >/dev/null
run_json queue replay --limit 40 >/dev/null
if "${BIN}" --root "${ROOT_DIR}" read "${CRUD_URI}" >/dev/null 2>&1; then
  echo "crud delete verification failed: file still readable (${CRUD_URI})" >&2
  exit 1
fi

crud_list_json="$(run_json ls "${TARGET_URI}/manual-crud" --recursive)"
if echo "${crud_list_json}" | jq -e --arg uri "${CRUD_URI}" 'map(.uri) | any(. == $uri)' >/dev/null; then
  echo "crud delete verification failed: deleted URI still listed (${CRUD_URI})" >&2
  exit 1
fi

PASS_REASONS=()
if ! rate_meets_threshold "${FIND_NONEMPTY_RATE}" "${MIN_FIND_NONEMPTY_RATE}"; then
  PASS_REASONS+=("find_nonempty_rate<${MIN_FIND_NONEMPTY_RATE}")
fi
if ! rate_meets_threshold "${SEARCH_NONEMPTY_RATE}" "${MIN_SEARCH_NONEMPTY_RATE}"; then
  PASS_REASONS+=("search_nonempty_rate<${MIN_SEARCH_NONEMPTY_RATE}")
fi
if ! rate_meets_threshold "${FIND_TOP1_RATE}" "${MIN_FIND_TOP1_RATE}"; then
  PASS_REASONS+=("find_top1_rate<${MIN_FIND_TOP1_RATE}")
fi
if ! rate_meets_threshold "${SEARCH_TOP1_RATE}" "${MIN_SEARCH_TOP1_RATE}"; then
  PASS_REASONS+=("search_top1_rate<${MIN_SEARCH_TOP1_RATE}")
fi
if ! rate_meets_threshold "${FIND_TOP5_RATE}" "${MIN_FIND_TOP5_RATE}"; then
  PASS_REASONS+=("find_top5_rate<${MIN_FIND_TOP5_RATE}")
fi
if ! rate_meets_threshold "${SEARCH_TOP5_RATE}" "${MIN_SEARCH_TOP5_RATE}"; then
  PASS_REASONS+=("search_top5_rate<${MIN_SEARCH_TOP5_RATE}")
fi

STATUS="PASS"
if [[ "${#PASS_REASONS[@]}" -gt 0 ]]; then
  STATUS="FAIL"
fi

cp "${ROWS_FILE}" "${RAW_PATH}"

{
  echo "# Real Dataset Random Benchmark (contextSet)"
  echo
  echo "Date: ${REPORT_DATE}"
  echo "Seed: ${SEED}"
  echo "Root: \`${ROOT_DIR}\`"
  echo "Dataset: \`${DATASET_DIR}\`"
  echo "Target URI: \`${TARGET_URI}\`"
  echo "Raw data: \`${RAW_PATH}\`"
  echo
  echo "## Ingest"
  echo "- Status: ${add_status}"
  echo "- Recursive entries listed: ${entry_count}"
  echo "- Tree root: ${tree_root}"
  echo
  echo "## Random Retrieval Metrics"
  echo "- Sampled heading scenarios: ${TOTAL} (candidate headings: ${candidate_total})"
  echo "- Unique headings in sample: ${UNIQUE_HEADING_COUNT} (ambiguous duplicates: ${AMBIGUOUS_HEADING_COUNT})"
  echo "- search min-match filter applied scenarios: ${SEARCH_MIN_MATCH_APPLIED}/${TOTAL} (min-match-tokens=${MIN_MATCH_TOKENS})"
  echo "- find non-empty: ${FIND_NONEMPTY}/${TOTAL} (${FIND_NONEMPTY_RATE}%)"
  echo "- search non-empty: ${SEARCH_NONEMPTY}/${TOTAL} (${SEARCH_NONEMPTY_RATE}%)"
  echo "- find top1 expected-uri: ${FIND_TOP1}/${TOTAL} (${FIND_TOP1_RATE}%)"
  echo "- search top1 expected-uri: ${SEARCH_TOP1}/${TOTAL} (${SEARCH_TOP1_RATE}%)"
  echo "- find top5 expected-uri: ${FIND_TOP5}/${TOTAL} (${FIND_TOP5_RATE}%)"
  echo "- search top5 expected-uri: ${SEARCH_TOP5}/${TOTAL} (${SEARCH_TOP5_RATE}%)"
  echo
  echo "## Latency (ms)"
  echo "- find mean/p50/p95: ${FIND_MEAN_MS} / ${FIND_P50_MS} / ${FIND_P95_MS}"
  echo "- search mean/p50/p95: ${SEARCH_MEAN_MS} / ${SEARCH_P50_MS} / ${SEARCH_P95_MS}"
  echo
  echo "## CRUD Validation"
  echo "- Create uri: \`${CRUD_URI}\`"
  echo "- Create status: $(echo "${crud_create_json}" | jq -r '.status // "ok"')"
  echo "- Update status: $(echo "${crud_update_json}" | jq -r '.status // "ok"')"
  echo "- Read-back contains update token: pass (\`${CRUD_TOKEN_UPDATE}\`)"
  echo "- Delete check: pass (not readable, not listed)"
  echo
  echo "## Thresholds"
  echo "- min find non-empty rate: ${MIN_FIND_NONEMPTY_RATE}%"
  echo "- min search non-empty rate: ${MIN_SEARCH_NONEMPTY_RATE}%"
  echo "- min find top1 rate: ${MIN_FIND_TOP1_RATE}%"
  echo "- min search top1 rate: ${MIN_SEARCH_TOP1_RATE}%"
  echo "- min find top5 rate: ${MIN_FIND_TOP5_RATE}%"
  echo "- min search top5 rate: ${MIN_SEARCH_TOP5_RATE}%"
  echo
  echo "## Sample Rows"
  echo
  echo "| file | heading | find_hits | search_hits | find_top1 | find_top5 | search_top1 | search_top5 | find_latency_ms | search_latency_ms |"
  echo "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|"
} >"${REPORT_PATH}"

# Render the sample table with explicit field extraction.
{
  while IFS=$'\t' read -r file _uri heading find_hits search_hits find_top1 find_top5 search_top1 search_top5 find_latency_ms search_latency_ms; do
    md_file="${file//|/\\|}"
    md_heading="${heading//|/\\|}"
    echo "| \`${md_file}\` | ${md_heading} | ${find_hits} | ${search_hits} | ${find_top1} | ${find_top5} | ${search_top1} | ${search_top5} | ${find_latency_ms} | ${search_latency_ms} |"
  done <"${ROWS_FILE}"
} >>"${REPORT_PATH}"

{
  echo
  echo "## Verdict"
  echo "- Status: ${STATUS}"
  if [[ "${#PASS_REASONS[@]}" -eq 0 ]]; then
    echo "- Reasons: none"
  else
    echo "- Reasons:"
    for reason in "${PASS_REASONS[@]}"; do
      echo "  - ${reason}"
    done
  fi
} >>"${REPORT_PATH}"

echo "Report: ${REPORT_PATH}"
echo "Raw: ${RAW_PATH}"
echo "Status: ${STATUS}"

if [[ "${STATUS}" != "PASS" ]]; then
  exit 1
fi
