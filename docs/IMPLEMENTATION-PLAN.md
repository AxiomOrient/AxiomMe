# Implementation Plan

Date: 2026-02-26
Scope: `/Users/axient/repository/AxiomMe` (planning artifacts sync + release publication checkpoint remediation for tag-push CI)

## Objective

Establish the canonical planning artifacts required by the implementation workflow, keep mirror-notice readiness closure intact, and resolve release publication checkpoint failures on tag-push CI.

## Inputs

1. `docs/ONTOLOGY_LAYER_IMPLEMENTATION_PLAN_2026-02-23.md`
2. Current dirty worktree status from `git status --short`

## Constraints

1. Do not revert or rewrite unrelated pre-existing changes.
2. Keep this run scoped to planning artifact bootstrap and synchronization.
3. Preserve deterministic task routing via explicit `TASK-ID` rows.

## Verification Map (Narrow -> Broader)

1. Narrow: file existence and markdown structure for `docs/IMPLEMENTATION-PLAN.md` and `docs/TASKS.md`.
2. Medium: task-state consistency check against `docs/ONTOLOGY_LAYER_IMPLEMENTATION_PLAN_2026-02-23.md` status section.
3. Broader: run `bash scripts/quality_gates.sh` and capture output evidence after active dirty-tree implementation stabilizes.

## Acceptance Criteria

1. `docs/IMPLEMENTATION-PLAN.md` exists and records objective, constraints, verification map, and task selection.
2. `docs/TASKS.md` exists and includes deterministic task rows with lifecycle evidence.
3. Next action mapping is valid as either one open task row (`TODO|BLOCKED`) or explicit `Selected For Next: NONE`.
4. Release readiness is marked final only after remote tag publication + tag-push CI evidence is captured.

## Task Selection for This Run

1. `TASK-001`: Bootstrap canonical plan artifact.
2. `TASK-002`: Bootstrap canonical task ledger and synchronize ontology baseline completion state.
3. `TASK-003`: Execute `scripts/quality_gates.sh` and collect fresh gate evidence.
4. `TASK-004`: Create post-notice tag, execute notice gate strict path, and rerun quality gates.
5. `TASK-005`: Apply one-cycle readiness closure updates to operations report/checklist based on `NX-009`.
6. `TASK-006`: Verify remote publication checkpoint (`push tag + CI evidence`) for release-go decision.
7. `TASK-007`: Apply CI remediation patch (`derivable_impls` fix + `rg` fallback hardening) and verify locally.
8. `TASK-008`: Publish remediation commit/tag and re-verify tag-push CI success.

## Post-Run Routing Update

1. Mirror notice gate unblock is completed (`TASK-004` `DONE`).
2. One-cycle readiness closure is completed (`TASK-005` `DONE`).
3. Release publication checkpoint remains blocked after first tag-push CI failure (`TASK-006` `BLOCKED`).
4. CI remediation patch is completed locally (`TASK-007` `DONE`).
5. Routed next action is publish+reverify (`NX-013 -> TASK-008` `TODO`).

## Data Model

1. `TaskRecord`: `task_id`, `status`, `priority`, `source`, `summary`, `evidence`.
2. `Status`: `TODO | DOING | DONE | BLOCKED`.
3. `Evidence`: file path or command output reference captured in `docs/TASKS.md`.

## Transformations vs Side Effects

1. Transformations:
   - Convert supplemental plan state into canonical task rows.
   - Normalize task status lifecycle records into deterministic transitions.
2. Side effects:
    - Write `docs/IMPLEMENTATION-PLAN.md`.
    - Write `docs/TASKS.md`.
    - Write `docs/MIRROR_MIGRATION_OPERATIONS_REPORT_2026-Q2.md`.
    - Create post-notice tag `0.1.3`.
    - Query GitHub Actions run state/logs/artifacts for tag-push CI evidence.
    - Execute `scripts/mirror_notice_gate.sh` (strict gate path) and refresh gate snapshot.
    - Execute `scripts/mirror_notice_router.sh` and refresh route snapshot.
    - Execute `bash scripts/quality_gates.sh` and refresh mirror notice artifacts.
    - Patch `crates/axiomme-core/src/models/benchmark.rs` and `scripts/check_prohibited_tokens.sh`.

## Perf Notes

1. This run performed one strict notice-gate execution and one full quality-gate execution; it did not introduce additional code-path changes in this run.
2. `TASK-004/005` execution confirmed notice gate `status=ready`, strict report pass (`unresolved_blockers=0`), and quality gates passed on current workspace state.
3. `TASK-006` attempt proved remote tag publication but failed tag-push CI (`run 22436388999`, `Run quality gates`).
4. `TASK-007` remediation passes local clippy + quality gates, but final go/no-go still depends on publishing these fixes (`TASK-008`).

## Risks and Rollback

1. Risk: task rows can become stale if large uncommitted changes evolve without updates.
2. Mitigation: when a new routed action appears, add a new `TASK-ID` mapping immediately in `docs/TASKS.md`.
3. Operational risk: remote tag publish or CI failure can invalidate local-ready verdict; keep `TASK-008` as P0 gate until new tag-push CI is green.
4. Rollback: revert this run's planning-doc edits and task-state transitions only.
