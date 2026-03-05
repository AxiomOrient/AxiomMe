# Implementation Plan

## Goal
- Reduce accidental complexity in large search/index units while preserving behavior and release safety.
- Close release-blocking OM contract/boundary gaps with minimal and explicit fixes.

## Scope
- `crates/axiomme-core/src/client/search/mod.rs`
- `crates/axiomme-core/src/client/search/snapshot.rs` (new)
- `crates/axiomme-core/src/client/search/telemetry.rs` (new)
- `crates/axiomme-core/src/index.rs`
- `crates/axiomme-core/src/client/mirror_outbox/reflector.rs`
- `crates/axiomme-core/src/client/mirror_outbox/reflector/tests.rs`
- `crates/axiomme-core/src/state/om/scope.rs`
- `crates/axiomme-core/src/state/tests.rs`
- `crates/axiomme-core/src/release_gate/contract_probe.rs`
- `crates/axiomme-core/src/release_gate/tests.rs`
- `docs/TASKS.md`

## Constraints
- No behavior changes for OM hint materialization, selection, or compaction.
- Keep episodic/AxiomMe boundary unchanged.
- Keep rollout/release gate scope small and explicit.
- All changes must pass `axiomme-core` tests and `clippy -D warnings`.

## Release Hardening Phase
1. OM Event Session Boundary
- Enforce `session_id` payload and event `uri` session segment consistency.
2. Reflector Strict Parser
- Reject structured contract payloads with unsupported schema instead of text fallback acceptance.
3. Continuation State Resolution
- If preferred thread id is requested but missing, return `None` instead of cross-thread fallback.
4. Prompt Contract Gate Baseline
- Prefer merge-base baseline when `.git` exists; fail closed when no comparable base is available.

## Data Model
- `SnapshotEntryInputs`: grouped snapshot assembly inputs (`fallback_thread_id`, reserved texts, buffered entries/chunk ids, selected count).
- `SnapshotVisibleEntrySelection`: explicit selection result set (`selected`, `activated`, `buffered`).
- `IndexDocumentPayload`: pure upsert-calculation payload for index ingestion (`exact_keys`, `text_lower`, `term_freq`, `doc_len`, `vector`).

## Execution Phases
1. Planning Artifacts
- Create `IMPLEMENTATION-PLAN.md` and `TASKS.md` with stable `TASK-ID` rows.
2. Search Snapshot Simplification
- Isolate snapshot-specific transformations into `search/snapshot.rs`.
- Keep state I/O in `AxiomMe` methods and pure transforms in snapshot module.
3. Index Upsert Simplification
- Keep `IndexDocumentPayload`-based upsert flow and ensure no score/index regression.
4. Search Orchestration Simplification
- Split `search_with_request` by isolating:
  - hint resolution (`resolve_search_hints`)
  - request-log side effects (`try_log_search_request`)
- Keep retrieval execution path and contract fields unchanged.
5. Search Telemetry Simplification
- Move query-plan visibility note assembly and request-detail JSON assembly to `search/telemetry.rs`.
- Keep field names and output contract unchanged.
6. Verification and Evidence
- Run focused tests, full `axiomme-core` tests, and clippy.
- Update `TASKS.md` with concrete command evidence.

## Verification Strategy
- Narrow tests:
  - `client::search::tests::snapshot_visible_entry_ids_dedupes_same_chunk_source_keeping_first_entry`
  - `index::tests::search_prioritizes_matching_doc`
- Broad test:
  - `cargo test -p axiomme-core --quiet`
- Static gate:
  - `cargo clippy -p axiomme-core --all-targets -- -D warnings`

## Risk/Rollback
- Risk: module extraction can introduce visibility/import breakage.
- Mitigation: keep public surface minimal (`pub(super)`), run targeted + full tests.
- Rollback: revert only touched files in this plan scope:
  - `crates/axiomme-core/src/client/search/mod.rs`
  - `crates/axiomme-core/src/client/search/snapshot.rs`
  - `crates/axiomme-core/src/client/search/telemetry.rs`
  - `crates/axiomme-core/src/index.rs`
  - `docs/IMPLEMENTATION-PLAN.md`
  - `docs/TASKS.md`
