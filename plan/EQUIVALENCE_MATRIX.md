# Replacement Equivalence Matrix

Date: 2026-02-10

Purpose:
- Prove replacement paths by behavior, failure-mode, and observability equivalence.

Status legend:
- `TODO`, `IN_PROGRESS`, `DONE`

## Matrix

| Area | Behavior Equivalence | Failure-Mode Equivalence | Observability Equivalence | Evidence | Status |
|---|---|---|---|---|---|
| Ingest stage -> finalize | staged tree matches source set and deterministic ordering | partial stage/finalize failure cleanup and restart safety | request log + outbox event consistency | `ingest_wait_and_replay_paths_are_behaviorally_equivalent`, `replay_outbox_recovers_after_restart_for_queued_ingest`, `ingest_failure_missing_source_cleans_temp_and_logs_error` | DONE |
| Tier synthesis (L0/L1) | abstract/overview generation stable for same input | malformed/empty directory handling | tier artifact presence and update logs | `tier_generation_is_deterministic_and_sorted`, `tier_generation_handles_empty_directory_and_observability`, `tier_generation_recovers_missing_artifact_after_drift_reindex`, `resolve_tier_synthesis_mode_defaults_to_deterministic`, `semantic_tier_synthesis_emits_summary_and_topics`, `ensure_directory_tiers_rewrites_when_directory_contents_change` | DONE |
| Replay/reconcile | replay outcome matches expected queue transitions | retry/dead-letter/backoff behavior preserved | checkpoint and diagnostics integrity | `replay_outbox_marks_event_done`, `replay_requeues_then_dead_letters_after_retry_budget`, `retry_backoff_is_deterministic_and_bounded`, `reconcile_prunes_missing_index_state`, `reconcile_dry_run_preserves_index_state`, `request_logs_support_operation_status_filters_case_insensitive` | DONE |
| Retrieval filter/limit parity | same filter and limit semantics across memory/qdrant/hybrid | backend fallback correctness | query plan notes + request logs + trace stop reason | search backend tests | DONE |
| URI and naming migration | canonical URI parse/format and scope semantics | invalid scope/traversal rejection | consistent runtime-visible protocol strings | uri tests + token scan | DONE |

## Exit Criteria

1. Every row reaches `DONE`.
2. Each row has at least one dedicated automated test per dimension.
3. No unresolved mismatch between behavior and observability evidence.
