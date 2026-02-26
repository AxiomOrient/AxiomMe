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
