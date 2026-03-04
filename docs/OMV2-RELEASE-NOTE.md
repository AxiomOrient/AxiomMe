# OM v2 Big-Bang Release Note

- Release ID: `OMV2-BB-2026-03-04`
- Release Date (KST): `2026-03-04`
- Strategy: `single-cutover big-bang`
- Compatibility Mode: `none` (no v1 fallback / no dual write)

## Scope
- AxiomMe OM runtime was fully cut over to v2-only paths.
- Legacy v1 continuation and reflection fallback paths were removed.
- Reflection apply now uses entry coverage (`covers_entry_ids`) as the single merge/apply contract.
- Search hint pipeline is locked to snapshot v2 + priority compaction.
- `episodic` protocol boundary is pinned to:
  - repo: `/Users/axient/repository/episodic`
  - rev: `9f4c075bf26b81c8a81fbb6539c46ec20ea8a181`

## Implemented Waves
1. Wave-1 (Protocol/Schema): OMV2-001~005 completed.
2. Wave-2 (Runtime/Retrieval): OMV2-006~010 completed.
3. Wave-3 (Destructive Cleanup/Gates): OMV2-011~014 completed.

## Gate Certification Summary
- Gate A: PASS
  - rev/lock contract and prompt contract/migration dry-run probes passed.
- Gate B: PASS
  - snapshot reader path, reflection CAS/idempotency, high-priority reservation checks passed.
- Gate C: PASS
  - deterministic fallback quality checks and full regression passed.

## Verification Evidence (Command Set)
- `cargo test -p axiomme-core`
- `cargo test`
- `cargo test` (in `/Users/axient/repository/episodic`)
- `cargo test -p axiomme-core release_gate::tests::episodic_manifest_req_contract_matches_requires_exact_git_rev -- --exact`
- `cargo test -p axiomme-core release_gate::tests::episodic_lock_version_contract_matches_checks_exact_version_shape -- --exact`
- `cargo test -p axiomme-core client::search::backend_tests::search_query_plan_notes_include_snapshot_reader_and_buffered_chunk_count -- --exact`
- `cargo test -p axiomme-core state::tests::om_reflection_apply_uses_generation_cas_and_event_idempotency -- --exact`
- `cargo test -p axiomme-core session::om::tests::deterministic_fallback_emits_current_task -- --exact`
- `cargo test -p axiomme-core session::om::tests::deterministic_fallback_preserves_error_context_identifiers -- --exact`
- `cargo test -p axiomme-core session::om::tests::deterministic_fallback_suppresses_low_confidence_suggested_response -- --exact`

## Breaking Changes
- No v1 compatibility layer is preserved.
- v1-style line-count-based reflection merge semantics are removed from runtime apply paths.
- Reflector response contract no longer carries `reflected_observation_line_count`.

## Operational Notes
- Rollback policy remains snapshot restore (code + DB snapshot), not compatibility fallback.
- Release should be considered valid only when checklist in `docs/OMV2-RELEASE-CHECKLIST.md` is fully completed and signed.
