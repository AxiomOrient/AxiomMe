#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=""
ROOT_CREATED=false
HISTORY_DIR=""
SNAPSHOT_PATH=""
BIN="${AXIOMME_BIN:-$(pwd)/target/debug/axiomme-cli}"
OUTPUT_PATH=""
MIN_SAMPLES=3
CONSECUTIVE_V2_CANDIDATE=3
FAIL_ON_TRIGGER=false

usage() {
  cat <<'EOF'
Usage:
  scripts/ontology_pressure_trend_gate.sh [options]

Options:
  --root <path>                     AxiomMe root directory for CLI execution (default: temporary)
  --history-dir <path>              Snapshot history directory (required)
  --snapshot <path>                 Snapshot JSON to append into history before evaluation (required)
  --axiomme-bin <path>              CLI binary path (default: target/debug/axiomme-cli)
  --output <path>                   Write trend JSON to file
  --min-samples <n>                 Minimum samples required before trigger evaluation (default: 3)
  --consecutive-v2-candidate <n>    Required consecutive tail `v2_candidate=true` count (default: 3)
  --fail-on-trigger <true|false>    Exit non-zero when trigger is true (default: false)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT_DIR="${2:-}"
      shift 2
      ;;
    --history-dir)
      HISTORY_DIR="${2:-}"
      shift 2
      ;;
    --snapshot)
      SNAPSHOT_PATH="${2:-}"
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
    --min-samples)
      MIN_SAMPLES="${2:-}"
      shift 2
      ;;
    --consecutive-v2-candidate)
      CONSECUTIVE_V2_CANDIDATE="${2:-}"
      shift 2
      ;;
    --fail-on-trigger)
      FAIL_ON_TRIGGER="${2:-}"
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

if [[ -z "$HISTORY_DIR" ]]; then
  echo "--history-dir is required" >&2
  exit 1
fi
if [[ -z "$SNAPSHOT_PATH" ]]; then
  echo "--snapshot is required" >&2
  exit 1
fi
if [[ ! -f "$SNAPSHOT_PATH" ]]; then
  echo "snapshot file not found: $SNAPSHOT_PATH" >&2
  exit 1
fi

if [[ -z "${AXIOMME_BIN:-}" ]]; then
  cargo build -p axiomme-cli >/dev/null
elif [[ ! -x "$BIN" ]]; then
  cargo build -p axiomme-cli >/dev/null
fi

if [[ -z "$ROOT_DIR" ]]; then
  ROOT_DIR="$(mktemp -d /tmp/axiomme-ontology-trend-XXXXXX)"
  ROOT_CREATED=true
fi

cleanup() {
  if [[ "$ROOT_CREATED" == true ]]; then
    rm -rf "$ROOT_DIR"
  fi
}
trap cleanup EXIT

mkdir -p "$HISTORY_DIR"

generated_at="$(jq -r '.generated_at_utc // "unknown"' "$SNAPSHOT_PATH")"
label="$(jq -r '.label // "snapshot"' "$SNAPSHOT_PATH")"
safe_generated_at="$(echo "$generated_at" | tr ':' '-' | tr -c 'A-Za-z0-9._-' '_')"
safe_label="$(echo "$label" | tr -c 'A-Za-z0-9._-' '_')"
history_file="$HISTORY_DIR/${safe_generated_at}__${safe_label}.json"
cp "$SNAPSHOT_PATH" "$history_file"

"$BIN" --root "$ROOT_DIR" init >/dev/null

trend_json="$(
  "$BIN" --root "$ROOT_DIR" ontology trend \
    --history-dir "$HISTORY_DIR" \
    --min-samples "$MIN_SAMPLES" \
    --consecutive-v2-candidate "$CONSECUTIVE_V2_CANDIDATE"
)"
echo "$trend_json" | jq -e . >/dev/null

if [[ -n "$OUTPUT_PATH" ]]; then
  mkdir -p "$(dirname "$OUTPUT_PATH")"
  printf '%s\n' "$trend_json" >"$OUTPUT_PATH"
fi

echo "$trend_json"
trigger="$(echo "$trend_json" | jq -r '.report.trigger_v2_design')"
if [[ "$FAIL_ON_TRIGGER" == "true" && "$trigger" == "true" ]]; then
  echo "ontology pressure trend triggered v2 design threshold" >&2
  exit 1
fi
echo "PASS: ontology pressure trend gate completed"
