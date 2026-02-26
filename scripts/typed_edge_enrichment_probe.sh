#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=""
ROOT_CREATED=false
BIN="${AXIOMME_BIN:-$(pwd)/target/debug/axiomme-cli}"
OUTPUT_PATH=""
ITERATIONS=12
DOC_COUNT=60
SEARCH_LIMIT=20
TARGET_URI="axiom://resources/typed-edge-probe"
QUERY_TEXT="typed edge latency probe"

usage() {
  cat <<'EOF'
Usage:
  scripts/typed_edge_enrichment_probe.sh [options]

Options:
  --root <path>             AxiomMe root directory (default: temporary)
  --axiomme-bin <path>      CLI binary path (default: target/debug/axiomme-cli)
  --output <path>           Write summary JSON to file
  --iterations <n>          Number of find runs per mode (default: 12)
  --doc-count <n>           Number of generated docs (default: 60)
  --search-limit <n>        find result limit (default: 20)
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
    --iterations)
      ITERATIONS="${2:-}"
      shift 2
      ;;
    --doc-count)
      DOC_COUNT="${2:-}"
      shift 2
      ;;
    --search-limit)
      SEARCH_LIMIT="${2:-}"
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

if [[ -z "${AXIOMME_BIN:-}" ]]; then
  cargo build -p axiomme-cli >/dev/null
elif [[ ! -x "$BIN" ]]; then
  cargo build -p axiomme-cli >/dev/null
fi

if [[ -z "$ROOT_DIR" ]]; then
  ROOT_DIR="$(mktemp -d /tmp/axiomme-typed-edge-XXXXXX)"
  ROOT_CREATED=true
fi

DATA_DIR="$(mktemp -d /tmp/axiomme-typed-edge-data-XXXXXX)"
TMP_DIR="$(mktemp -d /tmp/axiomme-typed-edge-run-XXXXXX)"

cleanup() {
  rm -rf "$DATA_DIR" "$TMP_DIR"
  if [[ "$ROOT_CREATED" == true ]]; then
    rm -rf "$ROOT_DIR"
  fi
}
trap cleanup EXIT

for i in $(seq -w 1 "$DOC_COUNT"); do
  cat >"$DATA_DIR/doc-$i.md" <<EOF
# Probe Document $i

$QUERY_TEXT document-$i
EOF
done

"$BIN" --root "$ROOT_DIR" init >/dev/null
"$BIN" --root "$ROOT_DIR" add "$DATA_DIR" --target "$TARGET_URI" --wait true >/dev/null

schema_path="$ROOT_DIR/agent/ontology/schema.v1.json"
mkdir -p "$(dirname "$schema_path")"
cat >"$schema_path" <<EOF
{
  "version": 1,
  "object_types": [{
    "id": "probe_doc",
    "uri_prefixes": ["$TARGET_URI"],
    "required_tags": [],
    "allowed_scopes": ["resources"]
  }],
  "link_types": [{
    "id": "depends_on",
    "from_types": ["probe_doc"],
    "to_types": ["probe_doc"],
    "min_arity": 2,
    "max_arity": $DOC_COUNT,
    "symmetric": true
  }],
  "action_types": [],
  "invariants": []
}
EOF

uri_values=()
for i in $(seq -w 1 "$DOC_COUNT"); do
  uri_values+=("\"$TARGET_URI/doc-$i.md\"")
done
uris_json="$(IFS=,; echo "${uri_values[*]}")"

relations_path="$ROOT_DIR/resources/typed-edge-probe/.relations.json"
cat >"$relations_path" <<EOF
[
  {
    "id": "depends_on",
    "uris": [$uris_json],
    "reason": "typed edge probe relation"
  }
]
EOF

run_mode() {
  local enabled="$1"
  local out_file="$TMP_DIR/mode_${enabled}.jsonl"
  : >"$out_file"
  for _ in $(seq 1 "$ITERATIONS"); do
    local result
    result="$(
      AXIOMME_SEARCH_TYPED_EDGE_ENRICHMENT="$enabled" \
      "$BIN" --root "$ROOT_DIR" find "$QUERY_TEXT" --target "$TARGET_URI" --limit "$SEARCH_LIMIT"
    )"
    echo "$result" | jq -e . >/dev/null
    local typed_note_count
    typed_note_count="$(echo "$result" | jq '[.query_plan.notes[]? | select(startswith("typed_edge_"))] | length')"
    if [[ "$enabled" == "1" && "$typed_note_count" -eq 0 ]]; then
      echo "typed-edge mode enabled but query plan note is missing" >&2
      exit 1
    fi
    if [[ "$enabled" == "0" && "$typed_note_count" -ne 0 ]]; then
      echo "typed-edge mode disabled but query plan note exists" >&2
      exit 1
    fi

    echo "$result" | jq -c '{
      latency_ms: (.trace.metrics.latency_ms // 0),
      relation_enriched_links: (.trace.metrics.relation_enriched_links // 0),
      typed_edge_links: (
        [.query_plan.notes[]? | select(startswith("typed_edge_links:")) | split(":")[1] | tonumber]
        | if length == 0 then 0 else max end
      ),
      typed_relation_count: (
        [.query_results[]?.relations[]? | select(.relation_type != null)] | length
      )
    }' >>"$out_file"
  done
}

summarize_mode() {
  local enabled="$1"
  local out_file="$TMP_DIR/mode_${enabled}.jsonl"
  jq -s '{
    runs: length,
    latency_ms: {
      min: (map(.latency_ms) | min),
      max: (map(.latency_ms) | max),
      avg: (if length == 0 then 0 else (map(.latency_ms) | add / length) end)
    },
    relation_enriched_links_avg: (if length == 0 then 0 else (map(.relation_enriched_links) | add / length) end),
    typed_edge_links_avg: (if length == 0 then 0 else (map(.typed_edge_links) | add / length) end),
    typed_relation_count_avg: (if length == 0 then 0 else (map(.typed_relation_count) | add / length) end)
  }' "$out_file"
}

run_mode "0"
run_mode "1"

disabled_json="$(summarize_mode "0")"
enabled_json="$(summarize_mode "1")"

summary_json="$(jq -n \
  --arg root_dir "$ROOT_DIR" \
  --arg target_uri "$TARGET_URI" \
  --arg query "$QUERY_TEXT" \
  --argjson iterations "$ITERATIONS" \
  --argjson doc_count "$DOC_COUNT" \
  --argjson disabled "$disabled_json" \
  --argjson enabled "$enabled_json" \
  '{
    root_dir: $root_dir,
    target_uri: $target_uri,
    query: $query,
    iterations: $iterations,
    doc_count: $doc_count,
    typed_edge_enrichment_disabled: $disabled,
    typed_edge_enrichment_enabled: $enabled,
    delta: {
      latency_ms_avg: ($enabled.latency_ms.avg - $disabled.latency_ms.avg),
      relation_enriched_links_avg: ($enabled.relation_enriched_links_avg - $disabled.relation_enriched_links_avg),
      typed_edge_links_avg: ($enabled.typed_edge_links_avg - $disabled.typed_edge_links_avg),
      typed_relation_count_avg: ($enabled.typed_relation_count_avg - $disabled.typed_relation_count_avg)
    }
  }')"

if [[ -n "$OUTPUT_PATH" ]]; then
  mkdir -p "$(dirname "$OUTPUT_PATH")"
  printf '%s\n' "$summary_json" >"$OUTPUT_PATH"
fi

echo "$summary_json"
echo "PASS: typed-edge enrichment probe completed"
