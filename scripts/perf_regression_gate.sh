#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=""
ROOT_CREATED=false
BIN="${AXIOMME_BIN:-$(pwd)/target/debug/axiomme-cli}"
OUTPUT_PATH=""
QUERY_LIMIT=120
SEARCH_LIMIT=10
THRESHOLD_P95_MS=600
MIN_TOP1_ACCURACY=0.75
WINDOW_SIZE=2
REQUIRED_PASSES=2
MIN_CASES=12
MAX_P95_REGRESSION_PCT=""
MAX_TOP1_REGRESSION_PCT=""

usage() {
  cat <<'EOF'
Usage:
  scripts/perf_regression_gate.sh [options]

Options:
  --root <path>                     AxiomMe root directory (default: temporary directory)
  --axiomme-bin <path>              CLI binary path (default: target/debug/axiomme-cli)
  --output <path>                   Write summary JSON to file
  --query-limit <n>                 benchmark query limit (default: 120)
  --search-limit <n>                benchmark search limit (default: 10)
  --threshold-p95-ms <n>            gate threshold p95 ms (default: 600)
  --min-top1-accuracy <float>       gate minimum top1 accuracy (default: 0.75)
  --window-size <n>                 benchmark gate window size (default: 2)
  --required-passes <n>             benchmark gate required passes (default: 2)
  --min-cases <n>                   minimum executed cases per run (default: 12)
  --max-p95-regression-pct <float>  optional p95 regression percentage limit
  --max-top1-regression-pct <float> optional top1 regression percentage limit
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT_DIR="${2:-}"
      shift 2
      ;;
    --axiomme-bin)
      BIN="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT_PATH="${2:-}"
      shift 2
      ;;
    --query-limit)
      QUERY_LIMIT="${2:-}"
      shift 2
      ;;
    --search-limit)
      SEARCH_LIMIT="${2:-}"
      shift 2
      ;;
    --threshold-p95-ms)
      THRESHOLD_P95_MS="${2:-}"
      shift 2
      ;;
    --min-top1-accuracy)
      MIN_TOP1_ACCURACY="${2:-}"
      shift 2
      ;;
    --window-size)
      WINDOW_SIZE="${2:-}"
      shift 2
      ;;
    --required-passes)
      REQUIRED_PASSES="${2:-}"
      shift 2
      ;;
    --min-cases)
      MIN_CASES="${2:-}"
      shift 2
      ;;
    --max-p95-regression-pct)
      MAX_P95_REGRESSION_PCT="${2:-}"
      shift 2
      ;;
    --max-top1-regression-pct)
      MAX_TOP1_REGRESSION_PCT="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

# Default local binary path should reflect latest source changes.
# If caller provides AXIOMME_BIN explicitly, respect it and only validate executability.
if [[ -z "${AXIOMME_BIN:-}" ]]; then
  cargo build -p axiomme-cli >/dev/null
elif [[ ! -x "$BIN" ]]; then
  cargo build -p axiomme-cli
fi

if [[ -z "$ROOT_DIR" ]]; then
  ROOT_DIR="$(mktemp -d /tmp/axiomme-perf-root-XXXXXX)"
  ROOT_CREATED=true
fi

DATA_DIR="$(mktemp -d /tmp/axiomme-perf-data-XXXXXX)"

cleanup() {
  rm -rf "$DATA_DIR"
  if [[ "$ROOT_CREATED" == true ]]; then
    rm -rf "$ROOT_DIR"
  fi
}
trap cleanup EXIT

run_json() {
  local out
  out="$("$BIN" --root "$ROOT_DIR" "$@")"
  echo "$out" | jq -e . >/dev/null
  printf '%s' "$out"
}

run_text() {
  "$BIN" --root "$ROOT_DIR" "$@"
}

cat >"$DATA_DIR/auth.md" <<'EOF'
# Auth

renamed-auth-vector
EOF

cat >"$DATA_DIR/database.md" <<'EOF'
# Database

cobalt-btree-sprout
EOF

cat >"$DATA_DIR/queue.md" <<'EOF'
# Queue

orbit-queue-latency
EOF

cat >"$DATA_DIR/trace.md" <<'EOF'
# Trace

helios-trace-window
EOF

cat >"$DATA_DIR/release.md" <<'EOF'
# Release

strict-gate-evidence
EOF

cat >"$DATA_DIR/session.md" <<'EOF'
# Session

aurora-session-memory
EOF

run_text init >/dev/null
run_json add "$DATA_DIR" --target axiom://resources/perf-regression >/dev/null

run_json eval golden add --query "renamed auth vector" --target axiom://resources/perf-regression --expected-top axiom://resources/perf-regression/auth.md >/dev/null
run_json eval golden add --query "cobalt btree sprout" --target axiom://resources/perf-regression --expected-top axiom://resources/perf-regression/database.md >/dev/null
run_json eval golden add --query "orbit queue latency" --target axiom://resources/perf-regression --expected-top axiom://resources/perf-regression/queue.md >/dev/null
run_json eval golden add --query "helios trace window" --target axiom://resources/perf-regression --expected-top axiom://resources/perf-regression/trace.md >/dev/null
run_json eval golden add --query "strict gate evidence" --target axiom://resources/perf-regression --expected-top axiom://resources/perf-regression/release.md >/dev/null
run_json eval golden add --query "aurora session memory" --target axiom://resources/perf-regression --expected-top axiom://resources/perf-regression/session.md >/dev/null

run1_json="$(run_json benchmark run --query-limit "$QUERY_LIMIT" --search-limit "$SEARCH_LIMIT" --include-golden true --include-trace false --include-stress true)"
run2_json="$(run_json benchmark run --query-limit "$QUERY_LIMIT" --search-limit "$SEARCH_LIMIT" --include-golden true --include-trace false --include-stress true)"

run1_cases="$(echo "$run1_json" | jq '.executed_cases')"
run2_cases="$(echo "$run2_json" | jq '.executed_cases')"
if [[ "$run1_cases" -lt "$MIN_CASES" || "$run2_cases" -lt "$MIN_CASES" ]]; then
  echo "insufficient benchmark case count: run1=${run1_cases}, run2=${run2_cases}, required=${MIN_CASES}" >&2
  exit 1
fi

gate_cmd=(
  benchmark gate
  --threshold-p95-ms "$THRESHOLD_P95_MS"
  --min-top1-accuracy "$MIN_TOP1_ACCURACY"
  --gate-profile perf-regression-nightly
  --window-size "$WINDOW_SIZE"
  --required-passes "$REQUIRED_PASSES"
  --record true
)
if [[ -n "$MAX_P95_REGRESSION_PCT" ]]; then
  gate_cmd+=(--max-p95-regression-pct "$MAX_P95_REGRESSION_PCT")
fi
if [[ -n "$MAX_TOP1_REGRESSION_PCT" ]]; then
  gate_cmd+=(--max-top1-regression-pct "$MAX_TOP1_REGRESSION_PCT")
fi
gate_json="$(run_json "${gate_cmd[@]}")"

summary_json="$(jq -n \
  --arg root_dir "$ROOT_DIR" \
  --argjson run1 "$run1_json" \
  --argjson run2 "$run2_json" \
  --argjson gate "$gate_json" \
  '{
    root_dir: $root_dir,
    run_1: {
      run_id: $run1.run_id,
      executed_cases: $run1.executed_cases,
      top1_accuracy: $run1.top1_accuracy,
      p95_latency_ms: $run1.p95_latency_ms,
      p95_latency_us: ($run1.p95_latency_us // null),
      report_uri: $run1.report_uri
    },
    run_2: {
      run_id: $run2.run_id,
      executed_cases: $run2.executed_cases,
      top1_accuracy: $run2.top1_accuracy,
      p95_latency_ms: $run2.p95_latency_ms,
      p95_latency_us: ($run2.p95_latency_us // null),
      report_uri: $run2.report_uri
    },
    gate: $gate
  }')"

if [[ -n "$OUTPUT_PATH" ]]; then
  mkdir -p "$(dirname "$OUTPUT_PATH")"
  printf '%s\n' "$summary_json" >"$OUTPUT_PATH"
fi

echo "$summary_json"
if [[ "$(echo "$gate_json" | jq -r '.passed')" != "true" ]]; then
  echo "performance regression gate failed" >&2
  exit 1
fi

echo "PASS: performance regression gate completed"
