# Implementation Plan

Date: 2026-02-26
Scope: `/Users/axient/repository/AxiomMe` (dev-branch-first integration, verification, and release-discipline correction)

## Objective

Correct the execution flow to a software-engineering-safe release model:

1. keep active development and verification on `dev`,
2. stop repeated tag/main release signaling during validation,
3. validate integrated changes on `dev` locally and in remote CI,
4. leave release decision gated by explicit feature-complete/UAT signoff.

## Inputs

1. `docs/FEATURE_SPEC.md`
2. `docs/TASKS.md`
3. Current branch/run state (`git status -sb`, `gh run list --workflow "Quality Gates"`)
4. User branch policy: development on `dev`, release only after completion and test signoff.

## Constraints

1. Do not revert unrelated existing work.
2. Do not create additional release tags during validation.
3. Keep `docs/TASKS.md` synchronized with real branch/CI evidence.
4. Release readiness must not be inferred from local-only checks.

## Verification Map (Narrow -> Broader)

1. Narrow: confirm fix commit integrity (`6b720ea`) and clean integration path onto `dev`.
2. Medium: run `bash scripts/quality_gates.sh` on `dev` and confirm full pass.
3. Broader: push `dev`, verify remote `Quality Gates` run success, and capture artifact evidence.
4. Final gate: feature-completeness/UAT signoff against `docs/FEATURE_SPEC.md` before release decision.

## Acceptance Criteria

1. `dev` contains latest validated code (`main` divergence closed by merge).
2. Local `quality_gates.sh` passes on `dev`.
3. Remote `dev` `Quality Gates` run is green with expected artifacts.
4. No additional release tag churn occurs during validation.
5. Release go/no-go remains blocked until feature-complete/UAT evidence exists.

## Task Selection for This Run

1. `TASK-009`: Enforce branch strategy correction and stop accidental main/tag validation churn.
2. `TASK-010`: Integrate latest work onto `dev`.
3. `TASK-011`: Execute local full quality gates on `dev`.
4. `TASK-012`: Validate remote `dev` CI and collect artifact evidence.
5. `TASK-013`: Keep release blocked until feature-complete/UAT signoff is produced.

## Post-Run Routing Update

1. Branch strategy correction is complete (`TASK-009` `DONE`).
2. `dev` integration is complete (`TASK-010` `DONE`, merge commit `f41b46e`).
3. Local quality verification is complete (`TASK-011` `DONE`).
4. Remote `dev` CI verification is complete (`TASK-012` `DONE`, run `22445209109` success).
5. Release remains intentionally pending on product-completion signoff (`TASK-013` `TODO`).

## Data Model

1. `TaskRecord`: `task_id`, `status`, `priority`, `source`, `action`, `evidence`.
2. `Status`: `TODO | DOING | DONE | BLOCKED`.
3. `RunEvidence`: `run_id`, `branch`, `conclusion`, `artifacts[]`.

## Transformations vs Side Effects

1. Transformations:
   - Re-route task state from tag-first to dev-first validation policy.
   - Normalize release decision input from CI-only to CI + feature/UAT signoff.
2. Side effects:
   - Cancel in-flight `main`/tag runs (`22443474696`, `22443480018`).
   - Delete accidental tag `0.1.6` on origin.
   - Merge `main` into `dev` (`f41b46e`).
   - Push `dev` and validate run `22445209109`.
   - Refresh gate evidence artifacts in `docs/` from local validation run.

## Perf Notes

1. Local `quality_gates.sh` on `dev` passed after integration.
2. Remote run `22445209109` jobs both passed:
   - `gates`: success
   - `release-pack-strict`: success
3. CI artifacts recorded:
   - `mirror-notice-gate` (`id=5673120116`)
   - `release-pack-strict-report` (`id=5673191406`)

## Risks and Rollback

1. Risk: release can still be mis-triggered if tag is created before feature/UAT signoff.
2. Mitigation: keep `TASK-013` as explicit P0 pre-release gate.
3. Risk: operational docs can drift from branch reality.
4. Mitigation: update `docs/TASKS.md` on every routing shift and CI conclusion.
5. Rollback: if needed, reset only this run's branch-strategy/doc edits; do not discard unrelated product code.
