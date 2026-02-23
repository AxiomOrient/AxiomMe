#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=""
ROOT_CREATED=false
WORKSPACE_DIR="$(pwd)"
BIN="${AXIOMME_BIN:-$(pwd)/target/debug/axiomme-cli}"
OUTPUT_PATH=""

REPLAY_LIMIT=20
REPLAY_MAX_CYCLES=2
TRACE_LIMIT=20
REQUEST_LIMIT=20
EVAL_TRACE_LIMIT=20
EVAL_QUERY_LIMIT=10
EVAL_SEARCH_LIMIT=5
BENCHMARK_QUERY_LIMIT=10
BENCHMARK_SEARCH_LIMIT=5
BENCHMARK_THRESHOLD_P95_MS=10000
BENCHMARK_MIN_TOP1_ACCURACY=0.0
BENCHMARK_WINDOW_SIZE=1
BENCHMARK_REQUIRED_PASSES=1

usage() {
  cat <<'EOF'
Usage:
  scripts/release_pack_strict_gate.sh [options]

Options:
  --root <path>                    AxiomMe root directory (default: temporary)
  --workspace-dir <path>           Workspace directory (default: current directory)
  --axiomme-bin <path>             CLI binary path (default: target/debug/axiomme-cli)
  --output <path>                  Write release pack report JSON to file
  --replay-limit <n>               Replay limit (default: 20)
  --replay-max-cycles <n>          Replay max cycles (default: 2)
  --trace-limit <n>                Trace limit (default: 20)
  --request-limit <n>              Request log limit (default: 20)
  --eval-trace-limit <n>           Eval trace limit (default: 20)
  --eval-query-limit <n>           Eval query limit (default: 10)
  --eval-search-limit <n>          Eval search limit (default: 5)
  --benchmark-query-limit <n>      Benchmark query limit (default: 10)
  --benchmark-search-limit <n>     Benchmark search limit (default: 5)
  --benchmark-threshold-p95-ms <n> Benchmark p95 threshold (default: 10000)
  --benchmark-min-top1-accuracy <f> Benchmark min top1 (default: 0.0)
  --benchmark-window-size <n>      Benchmark window size (default: 1)
  --benchmark-required-passes <n>  Benchmark required passes (default: 1)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT_DIR="${2:-}"
      shift 2
      ;;
    --workspace-dir)
      WORKSPACE_DIR="${2:-}"
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
    --replay-limit)
      REPLAY_LIMIT="${2:-}"
      shift 2
      ;;
    --replay-max-cycles)
      REPLAY_MAX_CYCLES="${2:-}"
      shift 2
      ;;
    --trace-limit)
      TRACE_LIMIT="${2:-}"
      shift 2
      ;;
    --request-limit)
      REQUEST_LIMIT="${2:-}"
      shift 2
      ;;
    --eval-trace-limit)
      EVAL_TRACE_LIMIT="${2:-}"
      shift 2
      ;;
    --eval-query-limit)
      EVAL_QUERY_LIMIT="${2:-}"
      shift 2
      ;;
    --eval-search-limit)
      EVAL_SEARCH_LIMIT="${2:-}"
      shift 2
      ;;
    --benchmark-query-limit)
      BENCHMARK_QUERY_LIMIT="${2:-}"
      shift 2
      ;;
    --benchmark-search-limit)
      BENCHMARK_SEARCH_LIMIT="${2:-}"
      shift 2
      ;;
    --benchmark-threshold-p95-ms)
      BENCHMARK_THRESHOLD_P95_MS="${2:-}"
      shift 2
      ;;
    --benchmark-min-top1-accuracy)
      BENCHMARK_MIN_TOP1_ACCURACY="${2:-}"
      shift 2
      ;;
    --benchmark-window-size)
      BENCHMARK_WINDOW_SIZE="${2:-}"
      shift 2
      ;;
    --benchmark-required-passes)
      BENCHMARK_REQUIRED_PASSES="${2:-}"
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

if [[ ! -d "$WORKSPACE_DIR" ]]; then
  echo "workspace directory not found: $WORKSPACE_DIR" >&2
  exit 1
fi

if [[ -z "${AXIOMME_BIN:-}" ]]; then
  cargo build -p axiomme-cli >/dev/null
elif [[ ! -x "$BIN" ]]; then
  cargo build -p axiomme-cli >/dev/null
fi

if [[ -z "$ROOT_DIR" ]]; then
  ROOT_DIR="$(mktemp -d /tmp/axiomme-release-gate-XXXXXX)"
  ROOT_CREATED=true
fi

cleanup() {
  if [[ "$ROOT_CREATED" == true ]]; then
    rm -rf "$ROOT_DIR"
  fi
}
trap cleanup EXIT

"$BIN" --root "$ROOT_DIR" init >/dev/null

report_json="$("$BIN" --root "$ROOT_DIR" release pack \
  --workspace-dir "$WORKSPACE_DIR" \
  --replay-limit "$REPLAY_LIMIT" \
  --replay-max-cycles "$REPLAY_MAX_CYCLES" \
  --trace-limit "$TRACE_LIMIT" \
  --request-limit "$REQUEST_LIMIT" \
  --eval-trace-limit "$EVAL_TRACE_LIMIT" \
  --eval-query-limit "$EVAL_QUERY_LIMIT" \
  --eval-search-limit "$EVAL_SEARCH_LIMIT" \
  --benchmark-query-limit "$BENCHMARK_QUERY_LIMIT" \
  --benchmark-search-limit "$BENCHMARK_SEARCH_LIMIT" \
  --benchmark-threshold-p95-ms "$BENCHMARK_THRESHOLD_P95_MS" \
  --benchmark-min-top1-accuracy "$BENCHMARK_MIN_TOP1_ACCURACY" \
  --benchmark-window-size "$BENCHMARK_WINDOW_SIZE" \
  --benchmark-required-passes "$BENCHMARK_REQUIRED_PASSES" \
  --security-audit-mode strict \
  --enforce)"

echo "$report_json" | jq -e '.passed == true' >/dev/null

if [[ -n "$OUTPUT_PATH" ]]; then
  mkdir -p "$(dirname "$OUTPUT_PATH")"
  printf '%s\n' "$report_json" >"$OUTPUT_PATH"
fi

echo "$report_json"
echo "PASS: strict release gate pack"
