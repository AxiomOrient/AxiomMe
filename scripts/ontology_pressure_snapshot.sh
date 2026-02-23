#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=""
ROOT_CREATED=false
WORKSPACE_DIR="$(pwd)"
BIN="${AXIOMME_BIN:-$(pwd)/target/debug/axiomme-cli}"
OUTPUT_PATH=""
LABEL="manual"
SCHEMA_FILE=""
MIN_ACTION_TYPES=3
MIN_INVARIANTS=3
MIN_ACTION_INVARIANT_TOTAL=5
MIN_LINK_TYPES_PER_OBJECT_BASIS_POINTS=15000

usage() {
  cat <<'EOF'
Usage:
  scripts/ontology_pressure_snapshot.sh [options]

Options:
  --root <path>                                 AxiomMe root directory (default: temporary)
  --workspace-dir <path>                        Workspace directory for git metadata (default: current directory)
  --axiomme-bin <path>                          CLI binary path (default: target/debug/axiomme-cli)
  --output <path>                               Write snapshot JSON to file
  --label <name>                                Snapshot label (default: manual)
  --schema-file <path>                          Override schema source file copied to agent/ontology/schema.v1.json
  --min-action-types <n>                        Pressure threshold (default: 3)
  --min-invariants <n>                          Pressure threshold (default: 3)
  --min-action-invariant-total <n>              Pressure threshold (default: 5)
  --min-link-types-per-object-basis-points <n> Pressure threshold (default: 15000)
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
    --label)
      LABEL="${2:-}"
      shift 2
      ;;
    --schema-file)
      SCHEMA_FILE="${2:-}"
      shift 2
      ;;
    --min-action-types)
      MIN_ACTION_TYPES="${2:-}"
      shift 2
      ;;
    --min-invariants)
      MIN_INVARIANTS="${2:-}"
      shift 2
      ;;
    --min-action-invariant-total)
      MIN_ACTION_INVARIANT_TOTAL="${2:-}"
      shift 2
      ;;
    --min-link-types-per-object-basis-points)
      MIN_LINK_TYPES_PER_OBJECT_BASIS_POINTS="${2:-}"
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
  ROOT_DIR="$(mktemp -d /tmp/axiomme-ontology-pressure-XXXXXX)"
  ROOT_CREATED=true
fi

cleanup() {
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

"$BIN" --root "$ROOT_DIR" init >/dev/null

schema_source_kind="bootstrap_default"
schema_source_path=""
if [[ -n "$SCHEMA_FILE" ]]; then
  if [[ ! -f "$SCHEMA_FILE" ]]; then
    echo "schema file not found: $SCHEMA_FILE" >&2
    exit 1
  fi
  schema_source_kind="external_file"
  schema_source_path="$(cd "$(dirname "$SCHEMA_FILE")" && pwd)/$(basename "$SCHEMA_FILE")"
  mkdir -p "$ROOT_DIR/agent/ontology"
  cp "$SCHEMA_FILE" "$ROOT_DIR/agent/ontology/schema.v1.json"
fi

validate_json="$(run_json ontology validate)"
pressure_json="$(
  run_json ontology pressure \
    --min-action-types "$MIN_ACTION_TYPES" \
    --min-invariants "$MIN_INVARIANTS" \
    --min-action-invariant-total "$MIN_ACTION_INVARIANT_TOTAL" \
    --min-link-types-per-object-basis-points "$MIN_LINK_TYPES_PER_OBJECT_BASIS_POINTS"
)"

git_sha="$(git -C "$WORKSPACE_DIR" rev-parse HEAD 2>/dev/null || true)"
git_branch="$(git -C "$WORKSPACE_DIR" rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
generated_at_utc="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

summary_json="$(jq -n \
  --arg generated_at_utc "$generated_at_utc" \
  --arg label "$LABEL" \
  --arg workspace_dir "$WORKSPACE_DIR" \
  --arg root_dir "$ROOT_DIR" \
  --arg root_created "$ROOT_CREATED" \
  --arg schema_source_kind "$schema_source_kind" \
  --arg schema_source_path "$schema_source_path" \
  --arg git_sha "$git_sha" \
  --arg git_branch "$git_branch" \
  --argjson validate "$validate_json" \
  --argjson pressure "$pressure_json" \
  '{
    generated_at_utc: $generated_at_utc,
    label: $label,
    workspace_dir: $workspace_dir,
    root: {
      path: (if $root_created == "true" then null else $root_dir end),
      ephemeral: ($root_created == "true")
    },
    git: {
      sha: $git_sha,
      branch: $git_branch
    },
    schema_source: {
      kind: $schema_source_kind,
      path: (if $schema_source_path == "" then null else $schema_source_path end)
    },
    validate: $validate,
    pressure: $pressure
  }')"

if [[ -n "$OUTPUT_PATH" ]]; then
  mkdir -p "$(dirname "$OUTPUT_PATH")"
  printf '%s\n' "$summary_json" >"$OUTPUT_PATH"
fi

echo "$summary_json"
echo "PASS: ontology pressure snapshot captured"
