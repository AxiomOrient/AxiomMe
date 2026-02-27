# Implementation Plan

Date: 2026-02-26
Scope: `/Users/axient/repository/AxiomMe` (worktree integration on `dev` + `TASK-013` feature-completeness/UAT gate)

## Objective

1. Integrate outstanding local worktree state into `dev` and origin.
2. Execute `TASK-013` as an explicit feature-completeness/UAT gate.
3. Run iterative self-critique and self-fix until the gate process is deterministic.
4. Produce clear go/no-go evidence and next unblock action.

## Inputs

1. `docs/FEATURE_SPEC.md`
2. `docs/TASKS.md`
3. Existing CI evidence (`run 22445209109`) and artifacts.
4. Manual validation harness: `scripts/manual_usecase_validation.sh`.

## Constraints

1. Keep dev-first branch policy (no new release tags during validation).
2. Do not revert unrelated product work.
3. Track status transitions/evidence in `docs/TASKS.md`.
4. If blocked, record owner/system and deterministic re-check command.

## Verification Map (Narrow -> Broader)

1. Narrow: ensure worktree integration is complete (`dev` pushed, stale stash removed).
2. Medium: execute manual usecase validation and ensure deterministic pass.
3. Broader: synthesize FR-by-FR matrix and scenario coverage in a gate document.
4. Final: decide go/no-go and map next action to one blocked/todo task.

## Acceptance Criteria

1. `dev` branch includes latest local documentation/integration commit and is pushed.
2. `scripts/manual_usecase_validation.sh` runs successfully and writes a fresh report.
3. `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md` exists with FR matrix, verdict, and signoff section.
4. `docs/TASKS.md` reflects `TASK-013` lifecycle and canonical next action mapping.

## Task Selection for This Run

1. `TASK-013`: Execute feature-completeness/UAT gate and generate signoff evidence document.
2. `TASK-014`: Track and block final release on unresolved environment/signoff dependency.

## Post-Run Routing Update

1. Worktree integration completed:
   - `dev` pushed (`52f5ca1`), stale stash dropped.
2. `TASK-013` completed with new gate artifact:
   - `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`.
3. Self-critique/self-fix loop completed on validation harness:
   - fixed eval JSON path bug,
   - corrected unsupported security mode usage,
   - corrected release-pack decision field parsing and null safety.
4. Release remains blocked under `TASK-014`:
   - missing `axiomme-webd`,
   - pending human UAT/release signoff.

## Data Model

1. `TaskRecord`: `task_id`, `status`, `priority`, `source`, `action`, `evidence`.
2. `GateRecord`: `gate_id`, `requirement`, `evidence`, `verdict`, `blocker`.
3. `BlockerRecord`: `owner`, `system`, `recheck_command`, `evidence_path`.

## Transformations vs Side Effects

1. Transformations:
   - Convert feature specification requirements into concrete FR/scenario evidence matrix.
   - Convert ad-hoc script failures into deterministic script behavior via iterative fixes.
2. Side effects:
   - Push `dev` branch update.
   - Drop obsolete stash entry.
   - Patch `scripts/manual_usecase_validation.sh`.
   - Generate `docs/MANUAL_USECASE_VALIDATION_2026-02-26.md`.
   - Generate `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`.
   - Update `docs/TASKS.md` lifecycle/next actions.

## Perf Notes

1. Manual usecase harness now completes end-to-end with `PASS`.
2. Remote CI reference remains green on `dev` (`22445209109`).
3. Remaining blocker is not performance-related; it is dependency/signoff gating.

## Risks and Rollback

1. Risk: release could be attempted without FR-011 runtime dependency installed.
2. Mitigation: keep `TASK-014` `BLOCKED` with deterministic re-check commands.
3. Risk: manual signoff omitted despite technical gate evidence.
4. Mitigation: require signoff fields in `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`.
5. Rollback: revert only this run's docs/script changes if policy changes; keep validated product code intact.

## Continuation Run (2026-02-27)

1. Target `TASK-ID`: `TASK-015` (Phase 1 zero-copy hardening + artifact synchronization).
2. Scope:
   - `crates/axiomme-core/src/retrieval/expansion.rs`
   - `docs/REFACTORING_TASKS.md`
   - `docs/TASKS.md`
3. Verification map:
   - Narrow: `cargo check -p axiomme-core --lib`
   - Medium: `cargo test -p axiomme-core --lib`
   - Broader: `cargo check --workspace --all-targets`
4. Expected outputs:
   - `Node` frontier propagation keeps `Arc<str>` without redundant `String` conversion.
   - Phase 1 completion is mapped to `TASK-ID` evidence.
   - Next action queue avoids blocked-loop re-selection.

## Continuation Run 2 (2026-02-27)

1. Target `TASK-ID`: `TASK-016` (Phase 2 decoupling from `LocalContextFs`).
2. Scope:
   - `crates/axiomme-core/src/fs.rs`
   - `crates/axiomme-core/src/relation_documents.rs`
   - `crates/axiomme-core/src/tier_documents.rs`
   - affected callers in `crates/axiomme-core/src/client/*` and `crates/axiomme-core/src/session/*`
3. Verification map:
   - Narrow: `cargo check -p axiomme-core --lib`
   - Medium: `cargo test -p axiomme-core --lib`
   - Broader: `cargo check --workspace --all-targets`
   - Full: `cargo test --workspace`
   - Security: `cargo audit -q`
4. Expected outputs:
   - `LocalContextFs` provides only generic filesystem primitives.
   - relation/tier domain read-write-validation logic is relocated outside `fs.rs`.
   - `docs/TASKS.md` and `docs/REFACTORING_TASKS.md` reflect `TASK-016` completion and next task routing.

## Continuation Run 3 (2026-02-27)

1. Target `TASK-ID`: `TASK-017` (Phase 3 Task 3.1 CLI leak removal for ontology enqueue).
2. Scope:
   - `crates/axiomme-core/src/client/ontology.rs`
   - `crates/axiomme-core/src/client.rs`
   - `crates/axiomme-cli/src/commands/mod.rs`
   - `crates/axiomme-core/src/client/tests/ontology_enqueue.rs`
3. Verification map:
   - Narrow: `cargo check -p axiomme-core --lib`
   - Medium: `cargo test -p axiomme-core --lib`
   - Broader: `cargo check --workspace --all-targets`
   - Full: `cargo test --workspace`
   - Security: `cargo audit -q`
4. Expected outputs:
   - ontology action enqueue orchestration (schema read/compile/validate + outbox enqueue) lives in core API.
   - CLI `OntologyCommand::ActionEnqueue` remains thin and delegates to core.
   - task/docs artifacts route next action to Phase 3 Task 3.2 (`TASK-018`/`NX-018`).

## Continuation Run 4 (2026-02-27)

1. Target `TASK-ID`: `TASK-018` (Phase 3 Task 3.2 client module rename cleanup).
2. Scope:
   - `crates/axiomme-core/src/client.rs`
   - top-level client modules under `crates/axiomme-core/src/client/*.rs` and companion submodule dirs
   - `docs/TASKS.md`
   - `docs/REFACTORING_TASKS.md`
3. Verification map:
   - Narrow: `cargo check -p axiomme-core --lib`
   - Medium: `cargo test -p axiomme-core --lib`
   - Broader: `cargo check --workspace --all-targets`
   - Full: `cargo test --workspace`
   - Security: `cargo audit -q`
4. Expected outputs:
   - `_service` suffix removed from top-level client modules/files with consistent module graph wiring.
   - no behavior change; test labels/module paths update only where implied by rename.
   - task/docs artifacts route next action to Phase 4 Task 4.1 (`TASK-019`/`NX-019`).

## Continuation Run 5 (2026-02-27)

1. Target `TASK-ID`: `TASK-019` (Phase 4 Task 4.1 strongly typed queue status).
2. Scope:
   - `crates/axiomme-core/src/models/queue.rs`
   - `crates/axiomme-core/src/models/mod.rs`
   - `crates/axiomme-core/src/state/queue.rs`
   - affected call sites/tests under `crates/axiomme-core/src/client/*`, `crates/axiomme-core/src/state/tests.rs`, `crates/axiomme-core/src/session/tests.rs`, and `crates/axiomme-cli/src/commands/tests.rs`
   - `docs/TASKS.md`
   - `docs/REFACTORING_TASKS.md`
3. Verification map:
   - Narrow: `cargo check -p axiomme-core --lib`
   - Medium: `cargo test -p axiomme-core --lib`
   - Broader: `cargo check --workspace --all-targets`
   - Full: `cargo test --workspace`
   - Security: `cargo audit -q`
4. Expected outputs:
   - `QueueEventStatus` enum replaces raw queue status string literals in model/state interfaces.
   - SQLite queue read/write path parses and emits typed status deterministically.
   - task/docs artifacts route next action to Phase 4 Task 4.2 (`TASK-020`/`NX-020`).

## Continuation Run 6 (2026-02-27)

1. Target `TASK-ID`: `TASK-020` (Phase 4 Task 4.2 structured pressure triggers).
2. Scope:
   - `crates/axiomme-core/src/ontology/pressure.rs`
   - `crates/axiomme-core/src/ontology/mod.rs`
   - compatibility call sites under `crates/axiomme-cli/src/commands/mod.rs` and tests
   - `docs/TASKS.md`
   - `docs/REFACTORING_TASKS.md`
3. Verification map:
   - Narrow: `cargo check -p axiomme-core --lib`
   - Medium: `cargo test -p axiomme-core --lib ontology::pressure::tests -- --nocapture`
   - Broader: `cargo check --workspace --all-targets`
   - Full: `cargo test --workspace`
   - Security: `cargo audit -q`
4. Expected outputs:
   - pressure trigger reasons are modeled as typed enum variants instead of ad-hoc formatted strings.
   - JSON/API contract for `trigger_reasons[]` remains string-array compatible through serialization/deserialization boundary.
   - task/docs artifacts route next action to release unblock path (`TASK-014`/`NX-021`).

## Continuation Run 7 (2026-02-27)

1. Target `TASK-ID`: `TASK-014` (release unblock evidence delta for FR-011 runtime dependency).
2. Scope:
   - `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`
   - `docs/TASKS.md`
   - `docs/IMPLEMENTATION-PLAN.md`
3. Verification map:
   - Narrow: `command -v axiomme-webd`
   - Medium: `AXIOMME_WEB_VIEWER_BIN=/Users/axient/repository/AxiomMe-web/target/debug/axiomme-webd target/debug/axiomme-cli --root <tmp-root> web --host 127.0.0.1 --port 8899` + `/api/fs/tree` probe
   - Broader: `cargo check --workspace --all-targets`
   - Full: `cargo test --workspace`
   - Security: `cargo audit -q`
4. Expected outputs:
   - FR-011 runtime dependency evidence is refreshed from "PATH binary missing" to "override-path probe pass".
   - `TASK-014` remains blocked only on human signoff.
   - task/docs artifacts route next action to signoff collection (`NX-022`).

## Continuation Run 8 (2026-02-27)

1. Target `TASK-ID`: `TASK-021` (prepare release signoff request packet).
2. Scope:
   - `docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`
   - `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`
   - `docs/TASKS.md`
   - `docs/IMPLEMENTATION-PLAN.md`
3. Verification map:
   - Narrow: ensure packet contains explicit final release decision fields.
   - Medium: confirm `TASK-014` blocker evidence points to packet + pending signoff rows.
   - Broader: `cargo check --workspace --all-targets`
   - Full: `cargo test --workspace`
   - Security: `cargo audit -q`
4. Expected outputs:
   - release owners get a deterministic signoff checklist artifact.
   - `TASK-014` remains blocked only on external approvals.
   - task/docs artifacts keep `NX-022` as selected next blocked action with refreshed evidence delta.

## Continuation Run 9 (2026-02-27)

1. Target `TASK-ID`: `TASK-022` (review finding verification + mergeability gate refresh).
2. Scope:
   - `crates/axiomme-core/src/retrieval/expansion.rs`
   - `crates/axiomme-core/src/index.rs`
   - `docs/TASKS.md`
   - `docs/IMPLEMENTATION-PLAN.md`
3. Verification map:
   - Narrow: confirm reviewer hotspots now type-check (`Node.uri`/frontier seed and `uri_path_prefix_match` tests).
   - Medium: `cargo check -p axiomme-core --lib` and `cargo test -p axiomme-core --lib`.
   - Broader: `cargo check --workspace --all-targets`.
   - Full: `cargo test --workspace`.
   - Security: `cargo audit -q`.
4. Expected outputs:
   - reviewer-reported compile mismatches are either fixed or disproven with current code evidence.
   - workspace mergeability gates are revalidated with fresh command evidence.
   - routing remains on external human signoff blocker (`NX-022`) with evidence delta.

## Continuation Run 10 (2026-02-27)

1. Target `TASK-ID`: `TASK-023` (signoff blocker loop-break via automated status probe).
2. Scope:
   - `scripts/release_signoff_status.sh`
   - `docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`
   - `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`
   - `docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`
   - `docs/TASKS.md`
   - `docs/IMPLEMENTATION-PLAN.md`
3. Verification map:
   - Narrow: run signoff status probe and verify deterministic blocked output while final release decision remains pending.
   - Medium: ensure gate/signoff docs reference the automated probe artifact and command.
   - Broader: `cargo check --workspace --all-targets`.
   - Full: `cargo test --workspace`.
   - Security: `cargo audit -q`.
4. Expected outputs:
   - human-signoff blocker has fresh machine-generated evidence delta instead of repeated static blocker text.
   - routing remains `NX-022` with updated external dependency evidence.
   - next run can deterministically detect unblock (`READY`) when signoff rows become `DONE`.

## Continuation Run 11 (2026-02-27)

1. Target `TASK-ID`: `TASK-024` (minimal NX-022 execution path).
2. Scope:
   - `scripts/record_release_signoff.sh`
   - `docs/TASKS.md`
   - `docs/IMPLEMENTATION-PLAN.md`
3. Verification map:
   - Narrow: shell syntax check (`bash -n scripts/record_release_signoff.sh`).
   - Medium: usage contract check (`scripts/record_release_signoff.sh --help`).
4. Expected outputs:
   - one command applies the final release decision and refreshes status artifact.
   - NX-022 blocker text becomes commandized input dependency (decision/name/date) only.

## Continuation Run 12 (2026-02-27)

1. Target `TASK-ID`: `TASK-025` (release-signoff flow simplification).
2. Scope:
   - `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`
   - `docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`
   - `docs/TASKS.md`
   - `docs/IMPLEMENTATION-PLAN.md`
   - `scripts/release_signoff_status.sh`
   - `scripts/record_release_signoff.sh`
3. Verification map:
   - Narrow: `scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`.
   - Medium: `scripts/record_release_signoff.sh --help`.
4. Expected outputs:
   - signoff model uses one final release decision (single owner).
   - NX-022 remains blocked only on one missing decision, not dual roles.

## Continuation Run 13 (2026-02-27)

1. Target `TASK-ID`: `TASK-014` (execute final release decision and close gate).
2. Scope:
   - `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`
   - `docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`
   - `docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`
   - `docs/TASKS.md`
3. Verification map:
   - Narrow: `scripts/record_release_signoff.sh --decision GO --name aiden`.
   - Medium: `scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md` (expect `rc=0` / `READY`).
4. Expected outputs:
   - `TASK-014` transitions to `DONE`.
   - `NX-022` transitions to `done` and queue selection becomes `NONE`.
