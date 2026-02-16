#!/usr/bin/env bash
set -euo pipefail

ROOT=""
WORKLOAD_CMD=""
AXIOMME_BIN="${AXIOMME_BIN:-axiomme-cli}"
REQUEST_LIMIT=500
THRESHOLD_P95_MS=800
MIN_TOP1_ACCURACY=0.70
WINDOW_SIZE=1
REQUIRED_PASSES=1
MAX_TOP1_REGRESSION=0.01
MIN_SEARCH_SAMPLES=20
MIN_BENCHMARK_CASES=1
MIN_GRADED_CASES=1
MAX_OM_DEAD_LETTER_RATE=0.02
OUTPUT_PATH=""

usage() {
  cat <<'EOF'
Usage:
  scripts/om_rollout_gate.sh \
    --workload-cmd "<command that generates search + benchmark data>" \
    [--root <axiomme root>] \
    [--axiomme-bin <path or command>] \
    [--request-limit <n>] \
    [--threshold-p95-ms <n>] \
    [--min-top1-accuracy <float>] \
    [--window-size <n>] \
    [--required-passes <n>] \
    [--max-top1-regression <float>] \
    [--min-search-samples <n>] \
    [--min-benchmark-cases <n>] \
    [--min-graded-cases <n>] \
    [--max-om-dead-letter-rate <float>] \
    [--output <json file>]

Notes:
  - Runs 3 rollout stages in order:
    1) baseline      (observer_model=0, reflector_model=0)
    2) observer_only (observer_model=1, reflector_model=0)
    3) full_model    (observer_model=1, reflector_model=1)
  - The workload command is executed once per stage with stage flags exported.
  - Stage metrics are read from:
    - benchmark gate output
    - search request logs (`trace requests`)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT="${2:-}"
      shift 2
      ;;
    --workload-cmd)
      WORKLOAD_CMD="${2:-}"
      shift 2
      ;;
    --axiomme-bin)
      AXIOMME_BIN="${2:-}"
      shift 2
      ;;
    --request-limit)
      REQUEST_LIMIT="${2:-}"
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
    --max-top1-regression)
      MAX_TOP1_REGRESSION="${2:-}"
      shift 2
      ;;
    --min-search-samples)
      MIN_SEARCH_SAMPLES="${2:-}"
      shift 2
      ;;
    --min-benchmark-cases)
      MIN_BENCHMARK_CASES="${2:-}"
      shift 2
      ;;
    --min-graded-cases)
      MIN_GRADED_CASES="${2:-}"
      shift 2
      ;;
    --max-om-dead-letter-rate)
      MAX_OM_DEAD_LETTER_RATE="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT_PATH="${2:-}"
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

if [[ -z "$WORKLOAD_CMD" ]]; then
  echo "--workload-cmd is required" >&2
  usage
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if ! command -v "$AXIOMME_BIN" >/dev/null 2>&1; then
  echo "axiomme binary not found: $AXIOMME_BIN" >&2
  exit 1
fi

run_axiomme() {
  if [[ -n "$ROOT" ]]; then
    "$AXIOMME_BIN" --root "$ROOT" "$@"
  else
    "$AXIOMME_BIN" "$@"
  fi
}

run_stage() {
  local stage_name="$1"
  local observer_model_enabled="$2"
  local reflector_model_enabled="$3"

  local stage_start
  stage_start="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  local gate_json
  gate_json="$(mktemp)"
  local request_before_json
  request_before_json="$(mktemp)"
  local request_after_json
  request_after_json="$(mktemp)"
  local queue_before_json
  queue_before_json="$(mktemp)"
  local queue_after_json
  queue_after_json="$(mktemp)"
  local benchmark_before_json
  benchmark_before_json="$(mktemp)"
  local benchmark_report_json
  benchmark_report_json="$(mktemp)"

  AXIOMME_OM_OBSERVER_MODEL_ENABLED="$observer_model_enabled" \
  AXIOMME_OM_REFLECTOR_MODEL_ENABLED="$reflector_model_enabled" \
  AXIOMME_OM_ROLLOUT_PROFILE="$stage_name" \
  run_axiomme trace requests \
    --limit "$REQUEST_LIMIT" \
    --operation search \
    --status ok \
    > "$request_before_json"

  AXIOMME_OM_OBSERVER_MODEL_ENABLED="$observer_model_enabled" \
  AXIOMME_OM_REFLECTOR_MODEL_ENABLED="$reflector_model_enabled" \
  AXIOMME_OM_ROLLOUT_PROFILE="$stage_name" \
  run_axiomme queue status \
    > "$queue_before_json"

  AXIOMME_OM_OBSERVER_MODEL_ENABLED="$observer_model_enabled" \
  AXIOMME_OM_REFLECTOR_MODEL_ENABLED="$reflector_model_enabled" \
  AXIOMME_OM_ROLLOUT_PROFILE="$stage_name" \
  run_axiomme benchmark list \
    --limit 1 \
    > "$benchmark_before_json"

  AXIOMME_OM_OBSERVER_MODEL_ENABLED="$observer_model_enabled" \
  AXIOMME_OM_REFLECTOR_MODEL_ENABLED="$reflector_model_enabled" \
  AXIOMME_OM_ROLLOUT_PROFILE="$stage_name" \
  AXIOMME_ROLLOUT_ROOT="$ROOT" \
  bash -lc "$WORKLOAD_CMD"

  AXIOMME_OM_OBSERVER_MODEL_ENABLED="$observer_model_enabled" \
  AXIOMME_OM_REFLECTOR_MODEL_ENABLED="$reflector_model_enabled" \
  AXIOMME_OM_ROLLOUT_PROFILE="$stage_name" \
  run_axiomme benchmark gate \
    --threshold-p95-ms "$THRESHOLD_P95_MS" \
    --min-top1-accuracy "$MIN_TOP1_ACCURACY" \
    --window-size "$WINDOW_SIZE" \
    --required-passes "$REQUIRED_PASSES" \
    > "$gate_json"

  AXIOMME_OM_OBSERVER_MODEL_ENABLED="$observer_model_enabled" \
  AXIOMME_OM_REFLECTOR_MODEL_ENABLED="$reflector_model_enabled" \
  AXIOMME_OM_ROLLOUT_PROFILE="$stage_name" \
  run_axiomme trace requests \
    --limit "$REQUEST_LIMIT" \
    --operation search \
    --status ok \
    > "$request_after_json"

  AXIOMME_OM_OBSERVER_MODEL_ENABLED="$observer_model_enabled" \
  AXIOMME_OM_REFLECTOR_MODEL_ENABLED="$reflector_model_enabled" \
  AXIOMME_OM_ROLLOUT_PROFILE="$stage_name" \
  run_axiomme queue status \
    > "$queue_after_json"

  local benchmark_report_uri
  benchmark_report_uri="$(jq -r '.latest.report_uri // empty' "$gate_json")"
  if [[ -z "$benchmark_report_uri" ]]; then
    echo "stage ${stage_name} missing latest benchmark report uri in gate output" >&2
    rm -f "$gate_json" "$request_before_json" "$request_after_json" "$queue_before_json" "$queue_after_json" "$benchmark_before_json" "$benchmark_report_json"
    exit 1
  fi

  local previous_benchmark_report_uri
  previous_benchmark_report_uri="$(jq -r '.[0].report_uri // empty' "$benchmark_before_json")"
  if [[ -n "$previous_benchmark_report_uri" && "$previous_benchmark_report_uri" == "$benchmark_report_uri" ]]; then
    echo "stage ${stage_name} benchmark gate did not produce a new report (still ${benchmark_report_uri})" >&2
    rm -f "$gate_json" "$request_before_json" "$request_after_json" "$queue_before_json" "$queue_after_json" "$benchmark_before_json" "$benchmark_report_json"
    exit 1
  fi

  AXIOMME_OM_OBSERVER_MODEL_ENABLED="$observer_model_enabled" \
  AXIOMME_OM_REFLECTOR_MODEL_ENABLED="$reflector_model_enabled" \
  AXIOMME_OM_ROLLOUT_PROFILE="$stage_name" \
  run_axiomme read "$benchmark_report_uri" > "$benchmark_report_json"

  local benchmark_cases
  benchmark_cases="$(jq '.executed_cases // 0' "$benchmark_report_json")"
  local graded_cases
  graded_cases="$(jq '[.results[]? | select(.expected_top_uri != null)] | length' "$benchmark_report_json")"

  if [[ "$benchmark_cases" -lt "$MIN_BENCHMARK_CASES" ]]; then
    echo "stage ${stage_name} has insufficient benchmark cases: ${benchmark_cases} < ${MIN_BENCHMARK_CASES}" >&2
    rm -f "$gate_json" "$request_before_json" "$request_after_json" "$queue_before_json" "$queue_after_json" "$benchmark_before_json" "$benchmark_report_json"
    exit 1
  fi

  if [[ "$graded_cases" -lt "$MIN_GRADED_CASES" ]]; then
    echo "stage ${stage_name} has insufficient graded benchmark cases: ${graded_cases} < ${MIN_GRADED_CASES}" >&2
    rm -f "$gate_json" "$request_before_json" "$request_after_json" "$queue_before_json" "$queue_after_json" "$benchmark_before_json" "$benchmark_report_json"
    exit 1
  fi

  local request_delta_json
  request_delta_json="$(jq -n \
    --argjson before "$(cat "$request_before_json")" \
    --argjson after "$(cat "$request_after_json")" '
    def id_map($rows):
      reduce ($rows // [])[] as $row
        ({}; .[$row.request_id] = true);
    (id_map($before)) as $before_ids
    | ([
        ($after // [])[]
        | select(($before_ids[.request_id] // false) | not)
        | (.details // {}) as $details
        | select(
            ($details.context_tokens_before_om | type) == "number"
            and ($details.context_tokens_after_om | type) == "number"
          )
        | {
            before: $details.context_tokens_before_om,
            after: $details.context_tokens_after_om
          }
      ]) as $rows
    | {
        sample_count: ($rows | length),
        avg_before: (
          if ($rows | length) == 0 then
            0
          else
            ([$rows[] | .before] | add / length)
          end
        ),
        avg_after: (
          if ($rows | length) == 0 then
            0
          else
            ([$rows[] | .after] | add / length)
          end
        )
      }
  ')"
  local sample_count
  sample_count="$(printf '%s\n' "$request_delta_json" | jq '.sample_count')"

  if [[ "$sample_count" -lt "$MIN_SEARCH_SAMPLES" ]]; then
    echo "stage ${stage_name} has insufficient search samples in stage delta: ${sample_count} < ${MIN_SEARCH_SAMPLES}" >&2
    rm -f "$gate_json" "$request_before_json" "$request_after_json" "$queue_before_json" "$queue_after_json" "$benchmark_before_json" "$benchmark_report_json"
    exit 1
  fi

  local avg_before
  avg_before="$(printf '%s\n' "$request_delta_json" | jq '.avg_before')"

  local avg_after
  avg_after="$(printf '%s\n' "$request_delta_json" | jq '.avg_after')"

  local om_dead_letter_delta_json
  om_dead_letter_delta_json="$(jq -n \
    --argjson before "$(cat "$queue_before_json")" \
    --argjson after "$(cat "$queue_after_json")" '
    def rate_map($items):
      reduce ($items // [])[] as $item
        ({}; .[$item.event_type] = {
          total: ($item.total // 0),
          dead_letter: ($item.dead_letter // 0)
        });
    (rate_map($before.queue_dead_letter_rate)) as $before_map
    | (rate_map($after.queue_dead_letter_rate)) as $after_map
    | ([($before_map + $after_map) | keys[] | select(startswith("om_"))] | unique) as $event_types
    | if ($event_types | length) == 0 then
        {
          max_rate: 0,
          total_delta: 0,
          dead_letter_delta: 0
        }
      else
        ([
          $event_types[]
          | . as $event_type
          | (($after_map[$event_type].total // 0) - ($before_map[$event_type].total // 0)) as $total_delta_raw
          | (($after_map[$event_type].dead_letter // 0) - ($before_map[$event_type].dead_letter // 0)) as $dead_delta_raw
          | {
              event_type: $event_type,
              total_delta: (if $total_delta_raw > 0 then $total_delta_raw else 0 end),
              dead_letter_delta: (if $dead_delta_raw > 0 then $dead_delta_raw else 0 end)
            }
        ]) as $delta
        | {
            max_rate: (
              [$delta[] | if .total_delta == 0 then 0 else (.dead_letter_delta / .total_delta) end]
              | max
            ),
            total_delta: ([$delta[] | .total_delta] | add),
            dead_letter_delta: ([$delta[] | .dead_letter_delta] | add)
          }
      end
  ')"
  local max_dead_letter_rate
  max_dead_letter_rate="$(printf '%s\n' "$om_dead_letter_delta_json" | jq '.max_rate')"
  local om_event_total_delta
  om_event_total_delta="$(printf '%s\n' "$om_dead_letter_delta_json" | jq '.total_delta')"
  local om_dead_letter_delta
  om_dead_letter_delta="$(printf '%s\n' "$om_dead_letter_delta_json" | jq '.dead_letter_delta')"

  local stage_json
  stage_json="$(jq -n \
    --arg stage "$stage_name" \
    --arg start "$stage_start" \
    --argjson observer_model_enabled "$observer_model_enabled" \
    --argjson reflector_model_enabled "$reflector_model_enabled" \
    --argjson gate "$(cat "$gate_json")" \
    --argjson sample_count "$sample_count" \
    --argjson benchmark_cases "$benchmark_cases" \
    --argjson graded_cases "$graded_cases" \
    --argjson avg_before "$avg_before" \
    --argjson avg_after "$avg_after" \
    --argjson om_event_total_delta "$om_event_total_delta" \
    --argjson om_dead_letter_delta "$om_dead_letter_delta" \
    --argjson max_dead_letter_rate "$max_dead_letter_rate" '
    {
      stage: $stage,
      stage_start: $start,
      observer_model_enabled: ($observer_model_enabled == 1),
      reflector_model_enabled: ($reflector_model_enabled == 1),
      gate_passed: ($gate.passed // false),
      top1_accuracy: ($gate.latest.top1_accuracy // 0),
      p95_latency_ms: ($gate.latest.p95_latency_ms // 0),
      search_samples: $sample_count,
      benchmark_cases: $benchmark_cases,
      graded_cases: $graded_cases,
      avg_context_tokens_before_om: $avg_before,
      avg_context_tokens_after_om: $avg_after,
      om_event_total_delta: $om_event_total_delta,
      om_dead_letter_delta: $om_dead_letter_delta,
      max_om_dead_letter_rate: $max_dead_letter_rate,
      context_efficiency_ratio:
        (if $avg_before == 0 then 0 else ($avg_after / $avg_before) end)
    }
  ')"

  rm -f "$gate_json" "$request_before_json" "$request_after_json" "$queue_before_json" "$queue_after_json" "$benchmark_before_json" "$benchmark_report_json"
  printf '%s\n' "$stage_json"
}

baseline_stage="$(run_stage "baseline" 0 0)"
observer_stage="$(run_stage "observer_only" 1 0)"
full_stage="$(run_stage "full_model" 1 1)"

summary="$(jq -n \
  --argjson baseline "$baseline_stage" \
  --argjson observer "$observer_stage" \
  --argjson full "$full_stage" \
  --argjson max_reg "$MAX_TOP1_REGRESSION" \
  --argjson min_top1 "$MIN_TOP1_ACCURACY" \
  --argjson threshold_p95 "$THRESHOLD_P95_MS" \
  --argjson min_samples "$MIN_SEARCH_SAMPLES" \
  --argjson min_benchmark_cases "$MIN_BENCHMARK_CASES" \
  --argjson min_graded_cases "$MIN_GRADED_CASES" \
  --argjson max_dead_letter_rate "$MAX_OM_DEAD_LETTER_RATE" '
  {
    stages: [$baseline, $observer, $full],
    checks: {
      benchmark_gate_all_passed:
        ($baseline.gate_passed and $observer.gate_passed and $full.gate_passed),
      baseline_min_samples:
        ($baseline.search_samples >= $min_samples),
      observer_min_samples:
        ($observer.search_samples >= $min_samples),
      full_min_samples:
        ($full.search_samples >= $min_samples),
      baseline_min_benchmark_cases:
        ($baseline.benchmark_cases >= $min_benchmark_cases),
      observer_min_benchmark_cases:
        ($observer.benchmark_cases >= $min_benchmark_cases),
      full_min_benchmark_cases:
        ($full.benchmark_cases >= $min_benchmark_cases),
      baseline_min_graded_cases:
        ($baseline.graded_cases >= $min_graded_cases),
      observer_min_graded_cases:
        ($observer.graded_cases >= $min_graded_cases),
      full_min_graded_cases:
        ($full.graded_cases >= $min_graded_cases),
      baseline_dead_letter_bounded:
        ($baseline.max_om_dead_letter_rate <= $max_dead_letter_rate),
      observer_dead_letter_bounded:
        ($observer.max_om_dead_letter_rate <= $max_dead_letter_rate),
      full_dead_letter_bounded:
        ($full.max_om_dead_letter_rate <= $max_dead_letter_rate),
      observer_top1_non_regression:
        (($baseline.top1_accuracy - $observer.top1_accuracy) <= $max_reg),
      full_top1_non_regression:
        (($baseline.top1_accuracy - $full.top1_accuracy) <= $max_reg),
      observer_efficiency_improved:
        (
          if $baseline.avg_context_tokens_before_om == 0 then
            true
          else
            ($observer.context_efficiency_ratio < $baseline.context_efficiency_ratio)
          end
        ),
      full_efficiency_improved:
        (
          if $baseline.avg_context_tokens_before_om == 0 then
            true
          else
            ($full.context_efficiency_ratio < $baseline.context_efficiency_ratio)
          end
        )
    }
  }
  | . + {
      rollout_passed:
        (
          .checks.benchmark_gate_all_passed
          and .checks.baseline_min_samples
          and .checks.observer_min_samples
          and .checks.full_min_samples
          and .checks.baseline_min_benchmark_cases
          and .checks.observer_min_benchmark_cases
          and .checks.full_min_benchmark_cases
          and .checks.baseline_min_graded_cases
          and .checks.observer_min_graded_cases
          and .checks.full_min_graded_cases
          and .checks.baseline_dead_letter_bounded
          and .checks.observer_dead_letter_bounded
          and .checks.full_dead_letter_bounded
          and .checks.observer_top1_non_regression
          and .checks.full_top1_non_regression
          and .checks.observer_efficiency_improved
          and .checks.full_efficiency_improved
        ),
      thresholds: {
        threshold_p95_ms: $threshold_p95,
        min_top1_accuracy: $min_top1,
        max_top1_regression: $max_reg,
        min_search_samples: $min_samples,
        min_benchmark_cases: $min_benchmark_cases,
        min_graded_cases: $min_graded_cases,
        max_om_dead_letter_rate: $max_dead_letter_rate
      }
    }
')"

if [[ -n "$OUTPUT_PATH" ]]; then
  printf '%s\n' "$summary" > "$OUTPUT_PATH"
fi

printf '%s\n' "$summary"
