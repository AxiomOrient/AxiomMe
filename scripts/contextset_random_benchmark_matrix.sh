#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  scripts/contextset_random_benchmark_matrix.sh [options]

Options:
  --dataset <path>           Dataset root (default: \$AXIOMME_DATASET_DIR)
  --target-uri <axiom-uri>   Ingest target URI (default: axiom://resources/contextSet)
  --sample-size <n>          Random heading sample size per seed (default: 24)
  --seeds <csv>              Seed list (default: 4242,777,9001)
  --date <YYYY-MM-DD>        Report date label (default: today)
  --report-path <path>       Matrix report path
  --bin <path>               CLI binary path (default: <repo>/target/debug/axiomme-cli)
  --find-limit <n>           find limit per query (default: 5)
  --search-limit <n>         search limit per query (default: 5)
  --min-match-tokens <n>     min match tokens for search (default: 2)
  --min-find-nonempty-rate <pct>    pass threshold (default: 90)
  --min-search-nonempty-rate <pct>  pass threshold (default: 80)
  --min-find-top1-rate <pct>        pass threshold (default: 65)
  --min-search-top1-rate <pct>      pass threshold (default: 65)
  --min-find-top5-rate <pct>        pass threshold (default: 50)
  --min-search-top5-rate <pct>      pass threshold (default: 45)
  --skip-build               Skip cargo build
  -h, --help                 Show help
EOF
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
  local n="$1"
  local d="$2"
  awk -v n="${n}" -v d="${d}" 'BEGIN { if (d == 0) printf "0.00"; else printf "%.2f", (n * 100.0) / d }'
}

percentile_from_column() {
  local file="$1"
  local col="$2"
  local percentile="$3"
  local count rank
  count="$(cut -f"${col}" "${file}" | wc -l | tr -d ' ')"
  if [[ "${count}" -eq 0 ]]; then
    echo "0"
    return
  fi
  rank=$(( (count * percentile + 99) / 100 ))
  cut -f"${col}" "${file}" | sort -n | sed -n "${rank}p"
}

REPORT_DATE="$(date +%F)"
DATASET_DIR="${AXIOMME_DATASET_DIR:-}"
TARGET_URI="axiom://resources/contextSet"
SAMPLE_SIZE=24
SEEDS_CSV="4242,777,9001"
REPORT_PATH=""
SKIP_BUILD=false
BENCH_SCRIPT="${SCRIPT_DIR}/contextset_random_benchmark.sh"
BIN="${REPO_ROOT}/target/debug/axiomme-cli"
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
    --seeds)
      SEEDS_CSV="${2:-}"
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
    --bin)
      BIN="${2:-}"
      shift 2
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
    --skip-build)
      SKIP_BUILD=true
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

if [[ ! -x "${BENCH_SCRIPT}" ]]; then
  echo "benchmark script not executable: ${BENCH_SCRIPT}" >&2
  exit 1
fi
if [[ ! -d "${DATASET_DIR}" ]]; then
  if [[ -z "${DATASET_DIR}" ]]; then
    echo "dataset is required (--dataset <path> or AXIOMME_DATASET_DIR)" >&2
    exit 1
  fi
  echo "dataset not found: ${DATASET_DIR}" >&2
  exit 1
fi

validate_integer_ge "sample-size" "${SAMPLE_SIZE}" 1
validate_integer_ge "find-limit" "${FIND_LIMIT}" 1
validate_integer_ge "search-limit" "${SEARCH_LIMIT}" 1
validate_integer_ge "min-match-tokens" "${MIN_MATCH_TOKENS}" 2
validate_percent_threshold "min-find-nonempty-rate" "${MIN_FIND_NONEMPTY_RATE}"
validate_percent_threshold "min-search-nonempty-rate" "${MIN_SEARCH_NONEMPTY_RATE}"
validate_percent_threshold "min-find-top1-rate" "${MIN_FIND_TOP1_RATE}"
validate_percent_threshold "min-search-top1-rate" "${MIN_SEARCH_TOP1_RATE}"
validate_percent_threshold "min-find-top5-rate" "${MIN_FIND_TOP5_RATE}"
validate_percent_threshold "min-search-top5-rate" "${MIN_SEARCH_TOP5_RATE}"

if [[ -z "${REPORT_PATH}" ]]; then
  REPORT_PATH="${REPO_ROOT}/logs/benchmarks/contextset_matrix.md"
fi
mkdir -p "$(dirname "${REPORT_PATH}")"
REPORT_DIR="$(cd "$(dirname "${REPORT_PATH}")" && pwd)"

if [[ "${SKIP_BUILD}" != "true" ]]; then
  cargo build -p axiomme-cli --manifest-path "${REPO_ROOT}/Cargo.toml" >/dev/null
fi
if [[ ! -x "${BIN}" ]]; then
  echo "cli binary not executable: ${BIN}" >&2
  exit 1
fi

ROWS_FILE="$(mktemp /tmp/axiomme-contextset-matrix-rows-XXXXXX.tsv)"
cleanup() {
  rm -f "${ROWS_FILE}"
}
trap cleanup EXIT

IFS=',' read -r -a SEEDS <<<"${SEEDS_CSV}"
if [[ "${#SEEDS[@]}" -eq 0 ]]; then
  echo "no seeds provided" >&2
  exit 1
fi

PASS_COUNT=0
TOTAL_COUNT=0

for seed in "${SEEDS[@]}"; do
  seed="$(echo "${seed}" | xargs)"
  if [[ -z "${seed}" ]]; then
    continue
  fi
  validate_integer_ge "seed" "${seed}" 0
  TOTAL_COUNT=$((TOTAL_COUNT + 1))
  seed_report_path="${REPORT_DIR}/contextset_seed_${seed}.md"
  seed_tsv_path="${seed_report_path%.md}.tsv"
  seed_start_epoch="$(date +%s)"
  echo "running seed ${seed} ..." >&2

  cmd=(
    "${BENCH_SCRIPT}"
    --dataset "${DATASET_DIR}"
    --target-uri "${TARGET_URI}"
    --sample-size "${SAMPLE_SIZE}"
    --seed "${seed}"
    --report-path "${seed_report_path}"
    --bin "${BIN}"
    --find-limit "${FIND_LIMIT}"
    --search-limit "${SEARCH_LIMIT}"
    --min-match-tokens "${MIN_MATCH_TOKENS}"
    --min-find-nonempty-rate "${MIN_FIND_NONEMPTY_RATE}"
    --min-search-nonempty-rate "${MIN_SEARCH_NONEMPTY_RATE}"
    --min-find-top1-rate "${MIN_FIND_TOP1_RATE}"
    --min-search-top1-rate "${MIN_SEARCH_TOP1_RATE}"
    --min-find-top5-rate "${MIN_FIND_TOP5_RATE}"
    --min-search-top5-rate "${MIN_SEARCH_TOP5_RATE}"
    --skip-build
  )

  seed_status="PASS"
  seed_reason="none"
  if ! "${cmd[@]}" >/dev/null; then
    seed_status="FAIL"
    seed_reason="benchmark_failed"
  fi
  seed_end_epoch="$(date +%s)"
  seed_duration_ms="$(( (seed_end_epoch - seed_start_epoch) * 1000 ))"

  if [[ ! -f "${seed_tsv_path}" ]]; then
    printf '%s\t%s\t0\t0\t0.00\t0.00\t0.00\t0.00\t0.00\t0.00\t0\t0\t%s\t%s\t%s\n' \
      "${seed}" "${seed_status}" "${seed_duration_ms}" "${seed_reason}" "${seed_report_path}" >>"${ROWS_FILE}"
    echo "seed ${seed}: ${seed_status} (${seed_duration_ms}ms) reason=${seed_reason}" >&2
    continue
  fi

  total="$(wc -l <"${seed_tsv_path}" | tr -d ' ')"
  unique="$(cut -f3 "${seed_tsv_path}" | sort -u | wc -l | tr -d ' ')"
  find_nonempty="$(awk -F'\t' '$4 + 0 > 0 { c++ } END { print c + 0 }' "${seed_tsv_path}")"
  search_nonempty="$(awk -F'\t' '$5 + 0 > 0 { c++ } END { print c + 0 }' "${seed_tsv_path}")"
  find_top1="$(awk -F'\t' '{ c += ($6 + 0) } END { print c + 0 }' "${seed_tsv_path}")"
  find_top5="$(awk -F'\t' '{ c += ($7 + 0) } END { print c + 0 }' "${seed_tsv_path}")"
  search_top1="$(awk -F'\t' '{ c += ($8 + 0) } END { print c + 0 }' "${seed_tsv_path}")"
  search_top5="$(awk -F'\t' '{ c += ($9 + 0) } END { print c + 0 }' "${seed_tsv_path}")"
  find_nonempty_rate="$(pct "${find_nonempty}" "${total}")"
  search_nonempty_rate="$(pct "${search_nonempty}" "${total}")"
  find_top1_rate="$(pct "${find_top1}" "${total}")"
  find_top5_rate="$(pct "${find_top5}" "${total}")"
  search_top1_rate="$(pct "${search_top1}" "${total}")"
  search_top5_rate="$(pct "${search_top5}" "${total}")"
  find_p95="$(percentile_from_column "${seed_tsv_path}" 10 95)"
  search_p95="$(percentile_from_column "${seed_tsv_path}" 11 95)"

  if [[ "${seed_status}" == "PASS" ]]; then
    PASS_COUNT=$((PASS_COUNT + 1))
  else
    parsed_reason="$(
      awk '
        /^## Verdict/ { in_verdict = 1; next }
        in_verdict && /^- Reasons: none/ { print "none"; found = 1; exit }
        in_verdict && /^  - / {
          sub(/^  - /, "", $0)
          reasons = reasons ? reasons "," $0 : $0
          found = 1
          next
        }
        in_verdict && /^## / { if (found) exit }
        END {
          if (found && reasons != "") print reasons
          else if (!found) print "benchmark_failed"
        }
      ' "${seed_report_path}"
    )"
    if [[ -n "${parsed_reason}" ]]; then
      seed_reason="${parsed_reason}"
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "${seed}" \
    "${seed_status}" \
    "${total}" \
    "${unique}" \
    "${find_nonempty_rate}" \
    "${search_nonempty_rate}" \
    "${find_top1_rate}" \
    "${find_top5_rate}" \
    "${search_top1_rate}" \
    "${search_top5_rate}" \
    "${find_p95}" \
    "${search_p95}" \
    "${seed_duration_ms}" \
    "${seed_reason}" \
    "${seed_report_path}" >>"${ROWS_FILE}"
  echo "seed ${seed}: ${seed_status} (${seed_duration_ms}ms) reason=${seed_reason}" >&2
done

if [[ "${TOTAL_COUNT}" -eq 0 ]]; then
  echo "no valid seeds to execute" >&2
  exit 1
fi

OVERALL_STATUS="PASS"
if [[ "${PASS_COUNT}" -ne "${TOTAL_COUNT}" ]]; then
  OVERALL_STATUS="FAIL"
fi

{
  echo "# Real Dataset Random Benchmark Matrix (contextSet)"
  echo
  echo "Date: ${REPORT_DATE}"
  echo "Dataset: \`${DATASET_DIR}\`"
  echo "Target URI: \`${TARGET_URI}\`"
  echo "Sample size per seed: ${SAMPLE_SIZE}"
  echo "Seeds: \`${SEEDS_CSV}\`"
  echo
  echo "## Summary"
  echo "- Passed seeds: ${PASS_COUNT}/${TOTAL_COUNT}"
  echo "- Overall status: ${OVERALL_STATUS}"
  echo "- Thresholds: find_nonempty>=${MIN_FIND_NONEMPTY_RATE}%, search_nonempty>=${MIN_SEARCH_NONEMPTY_RATE}%, find_top1>=${MIN_FIND_TOP1_RATE}%, search_top1>=${MIN_SEARCH_TOP1_RATE}%, find_top5>=${MIN_FIND_TOP5_RATE}%, search_top5>=${MIN_SEARCH_TOP5_RATE}%"
  echo
  echo "## Per-Seed Metrics"
  echo
  echo "| seed | status | scenarios | unique_headings | find_nonempty_rate | search_nonempty_rate | find_top1_rate | search_top1_rate | find_top5_rate | search_top5_rate | find_p95_ms | search_p95_ms | duration_ms | reason | report |"
  echo "|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|"
  while IFS=$'\t' read -r seed status total unique find_nonempty_rate search_nonempty_rate find_top1_rate find_top5_rate search_top1_rate search_top5_rate find_p95 search_p95 duration_ms reason report_path; do
    echo "| ${seed} | ${status} | ${total} | ${unique} | ${find_nonempty_rate}% | ${search_nonempty_rate}% | ${find_top1_rate}% | ${search_top1_rate}% | ${find_top5_rate}% | ${search_top5_rate}% | ${find_p95} | ${search_p95} | ${duration_ms} | ${reason} | \`${report_path}\` |"
  done <"${ROWS_FILE}"
} >"${REPORT_PATH}"

echo "Matrix report: ${REPORT_PATH}"
echo "Status: ${OVERALL_STATUS}"

if [[ "${OVERALL_STATUS}" != "PASS" ]]; then
  exit 1
fi
