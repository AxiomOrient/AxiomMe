# Tasks

Date: 2026-02-26 to 2026-02-27
Scope: dev-branch-first implementation, verification, and refactoring follow-up workflow

## Task Table

| TASK-ID | Status | Priority | Source | Action | Evidence |
| --- | --- | --- | --- | --- | --- |
| TASK-001 | DONE | P0 | ai | Create canonical `docs/IMPLEMENTATION-PLAN.md`. | `docs/IMPLEMENTATION-PLAN.md` |
| TASK-002 | DONE | P1 | merged | Create canonical `docs/TASKS.md` and sync ontology baseline completion from source plan. | `docs/TASKS.md`; `rg -n "Implementation Status \\(Current\\)|none for v1 baseline plan" docs/ONTOLOGY_LAYER_IMPLEMENTATION_PLAN_2026-02-23.md` |
| TASK-003 | DONE | P1 | ai | Run `bash scripts/quality_gates.sh` and capture gate evidence. | `bash scripts/quality_gates.sh` (exit 0); `docs/MIRROR_NOTICE_GATE_2026-02-24.json`; `docs/MIRROR_NOTICE_ROUTER_2026-02-24.json` |
| TASK-004 | DONE | P1 | merged | Resolve mirror notice follow-up by creating required post-notice tag and moving gate status to `ready`. | `git for-each-ref --sort=creatordate --format='%(refname:short) %(creatordate:short)' refs/tags`; notice gate snapshots in `docs/` |
| TASK-005 | DONE | P1 | merged | Complete one-cycle readiness closure (`NX-009`) in operations report/checklist. | `docs/MIRROR_MIGRATION_OPERATIONS_REPORT_2026-Q2.md`; `docs/MIRROR_NOTICE_ROUTER_2026-02-24.json` |
| TASK-006 | BLOCKED | P0 | ai | Verify old tag-based publication checkpoint (`0.1.3`) on CI. | `gh run view 22436388999` (`conclusion=failure`, step `Run quality gates`) |
| TASK-007 | DONE | P0 | ai | Fix CI root cause (`clippy::derivable_impls`) and harden prohibited-token scan fallback. | `crates/axiomme-core/src/models/benchmark.rs`; `scripts/check_prohibited_tokens.sh`; local clippy/gates pass evidence |
| TASK-008 | BLOCKED | P0 | merged | Tag-driven repeated validation flow. | Policy changed to dev-first validation; `gh run cancel 22443480018`; `gh run cancel 22443474696`; `git push origin --delete 0.1.6` |
| TASK-009 | DONE | P0 | ai | Enforce branch strategy correction: stop main/tag verification churn and continue on `dev`. | `gh run list --workflow "Quality Gates"` shows `22443480018` and `22443474696` as `completed/cancelled`; `git branch -vv` |
| TASK-010 | DONE | P0 | ai | Align `dev` with latest validated work from `main` and keep implementation on `dev`. | `git merge --no-ff main -m "chore: align dev with latest validated main changes"` -> merge commit `f41b46e` |
| TASK-011 | DONE | P0 | ai | Re-validate full quality gates locally on `dev` after merge. | `bash scripts/quality_gates.sh` (exit 0, 2026-02-26) |
| TASK-012 | DONE | P0 | ai | Validate remote CI on `dev` (no new tag) and capture artifact evidence. | `gh run view 22445209109` (`conclusion=success`, jobs `gates` + `release-pack-strict` success); `gh api .../runs/22445209109/artifacts` (`mirror-notice-gate`, `release-pack-strict-report`) |
| TASK-013 | DONE | P0 | merged | Execute feature-completeness/UAT gate and produce signoff record with verdict. | `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`; `docs/MANUAL_USECASE_VALIDATION_2026-02-26.md`; `scripts/manual_usecase_validation.sh` |
| TASK-014 | DONE | P0 | merged | Finalize release signoff with recorded final release decision after FR-011 runtime probe evidence update. | `scripts/record_release_signoff.sh --decision GO --name aiden`; `scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md` (`rc=0`); `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md` (`Final Release Decision: DONE`); `docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md` (`Overall: READY`) |
| TASK-015 | DONE | P0 | ai | Harden zero-copy retrieval frontier after `Arc<str>` migration and synchronize refactoring evidence docs. | `crates/axiomme-core/src/retrieval/expansion.rs`; `docs/REFACTORING_TASKS.md`; `cargo check -p axiomme-core --lib`; `cargo test -p axiomme-core --lib`; `cargo check --workspace --all-targets` |
| TASK-016 | DONE | P1 | merged | Complete Phase 2 decoupling: move relation/tier domain logic out of `LocalContextFs` into domain modules used by client/session layers. | `crates/axiomme-core/src/relation_documents.rs`; `crates/axiomme-core/src/tier_documents.rs`; updated call sites in `client/*` and `session/*`; `cargo test --workspace`; `cargo audit -q` |
| TASK-017 | DONE | P1 | merged | Complete Phase 3 Task 3.1: encapsulate ontology enqueue orchestration in core API and thin CLI handler. | `crates/axiomme-core/src/client/ontology.rs`; `crates/axiomme-cli/src/commands/mod.rs`; `crates/axiomme-core/src/client/tests/ontology_enqueue.rs`; `cargo test --workspace`; `cargo audit -q` |
| TASK-018 | DONE | P1 | merged | Complete Phase 3 Task 3.2: rename client `_service` modules to domain-focused module names and update module tree wiring. | renamed modules in `crates/axiomme-core/src/client.rs`; file moves in `crates/axiomme-core/src/client/*.rs`; `cargo test --workspace`; `cargo audit -q` |
| TASK-019 | DONE | P1 | merged | Complete Phase 4 Task 4.1: introduce strongly typed queue event status and migrate queue API signatures away from raw status strings. | `crates/axiomme-core/src/models/queue.rs`; `crates/axiomme-core/src/state/queue.rs`; queue call-site/test migrations in `crates/axiomme-core/src/client/*`, `crates/axiomme-core/src/state/tests.rs`, `crates/axiomme-core/src/session/tests.rs`; `cargo check --workspace --all-targets`; `cargo test --workspace`; `cargo audit -q` |
| TASK-020 | DONE | P1 | merged | Complete Phase 4 Task 4.2: replace ontology pressure magic string triggers with a structured trigger enum and keep string-array API contract via serialization boundary. | `crates/axiomme-core/src/ontology/pressure.rs`; `crates/axiomme-core/src/ontology/mod.rs`; `cargo check --workspace --all-targets`; `cargo test --workspace`; `cargo audit -q` |
| TASK-021 | DONE | P0 | merged | Prepare deterministic release signoff request packet for `TASK-014` owners with evidence links and completion checklist. | `docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`; updated references in `docs/TASKS.md` and `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md` |
| TASK-022 | DONE | P0 | ai | Validate reviewer-reported `Arc<str>` type mismatch regressions and re-run workspace build/quality gates. | reviewer targets: `crates/axiomme-core/src/retrieval/expansion.rs` + `crates/axiomme-core/src/index.rs`; `cargo check -p axiomme-core --lib`; `cargo test -p axiomme-core --lib`; `cargo check --workspace --all-targets`; `cargo test --workspace`; `cargo audit -q` |
| TASK-023 | DONE | P0 | merged | Add deterministic release-signoff status probe script and generate current status artifact for external approval follow-up. | `scripts/release_signoff_status.sh`; `scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md` (`rc=2`); `docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md` |
| TASK-024 | DONE | P0 | merged | Add minimal signoff-apply script to execute `NX-022` with explicit human decisions and auto-refresh status artifact. | `scripts/record_release_signoff.sh`; `scripts/record_release_signoff.sh --help`; `bash -n scripts/record_release_signoff.sh` |
| TASK-025 | DONE | P0 | merged | Simplify release signoff flow from dual-role approval to single final release decision model. | `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`; `docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`; `scripts/release_signoff_status.sh`; `scripts/record_release_signoff.sh` |
| TASK-026 | DONE | P0 | user | Cross-validate Gemini project analysis claim-by-claim against actual code, then produce corrected verdicts with executable evidence. | `cargo check -p axiomme-core --lib`; `cargo test -p axiomme-core --lib`; `cargo audit -q`; file evidence in `crates/axiomme-core/src/{fs.rs,index.rs,state/queue.rs,state/migration.rs,security_audit.rs,client.rs,client/ontology.rs,relation_documents.rs,tier_documents.rs}` |
| TASK-027 | DONE | P1 | ai | Remove remaining queue status/reconcile magic string literals from SQL aggregation and reconcile-run state paths by centralizing typed status constants/usages. | `crates/axiomme-core/src/models/reconcile.rs`; `crates/axiomme-core/src/client/queue_reconcile.rs`; `crates/axiomme-core/src/state/queue.rs`; `crates/axiomme-core/src/client/tests/relation_trace_logs.rs`; `cargo test -p axiomme-core --lib` |
| TASK-028 | DONE | P1 | ai | Add DB-level guardrails for queue/reconcile status domains (migration-time check/normalization strategy) to prevent invalid persisted statuses. | `crates/axiomme-core/src/state/migration.rs`; `crates/axiomme-core/src/state/tests.rs`; `cargo test -p axiomme-core --lib open_rejects_outbox_with_invalid_status_domain_value open_normalizes_whitespace_and_case_for_status_columns` |
| TASK-029 | DONE | P2 | ai | Reduce `InMemoryIndex::upsert` write-path allocation pressure (tokenization/text materialization) and preserve output compatibility. | `crates/axiomme-core/src/index.rs`; `cargo test -p axiomme-core --lib build_upsert_text_matches_legacy_join_shape_with_tags build_upsert_text_matches_legacy_join_shape_without_tags`; `cargo test -p axiomme-core --lib` |

## Lifecycle Log

1. `2026-02-26` `TASK-001` `TODO -> DOING -> DONE`
   - Evidence: canonical implementation plan file created.
2. `2026-02-26` `TASK-002` `TODO -> DOING -> DONE`
   - Evidence: canonical task ledger created and synchronized with ontology plan.
3. `2026-02-26` `TASK-003` `TODO -> DOING -> DONE`
   - Evidence: quality gates completed with pass and notice snapshots refreshed.
4. `2026-02-26` `TASK-004` `BLOCKED -> DOING -> DONE`
   - Evidence: notice gate moved to `ready/post_notice_tag_and_strict_gate_passed`.
5. `2026-02-26` `TASK-005` `TODO -> DOING -> DONE`
   - Evidence: operations report/checklist updated to one-cycle `ready`.
6. `2026-02-26` `TASK-006` `TODO -> DOING -> BLOCKED`
   - Blocker: tag `0.1.3` CI failed (`run 22436388999`).
7. `2026-02-26` `TASK-007` `TODO -> DOING -> DONE`
   - Evidence: clippy remediation and token-scan fallback merged; local gates pass.
8. `2026-02-26` `TASK-008` `TODO -> DOING -> BLOCKED`
   - Blocker: tag-proliferation flow rejected; accidental `0.1.6` signal rolled back.
9. `2026-02-26` `TASK-009` `TODO -> DOING -> DONE`
   - Evidence: in-progress `main/tag` runs canceled and branch strategy corrected.
10. `2026-02-26` `TASK-010` `TODO -> DOING -> DONE`
    - Evidence: `dev` now includes latest commits via merge commit `f41b46e`.
11. `2026-02-26` `TASK-011` `TODO -> DOING -> DONE`
    - Evidence: full local quality gates passed on `dev` after integration.
12. `2026-02-26` `TASK-012` `TODO -> DOING -> DONE`
    - Evidence: remote `dev` run `22445209109` completed `success` with two artifacts.
13. `2026-02-26` `TASK-013` `TODO -> DOING -> DONE`
    - Evidence: feature/UAT gate document generated and manual usecase validation script stabilized via iterative self-fix.
14. `2026-02-26` `TASK-014` `TODO -> DOING -> BLOCKED`
    - Blocker owner/system: `platform/tooling` (`axiomme-webd` dependency), `release-manager` (human signoff).
    - Deterministic re-check evidence:
      - `command -v axiomme-webd`
      - `target/debug/axiomme-cli --root <tmp-root> web --host 127.0.0.1 --port 8899`
      - updated signoff entry in `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`
15. `2026-02-27` `TASK-015` `TODO -> DOING -> DONE`
    - Evidence: retrieval frontier now carries `Arc<str>` in `Node`/visited/frontier propagation; local+workspace checks pass.
16. `2026-02-27` `TASK-016` `TODO -> DOING -> DONE`
    - Evidence: `LocalContextFs` no longer owns relation/tier read-write-validation logic; domain modules (`relation_documents`, `tier_documents`) now serve client/session flows; workspace tests and audit pass.
17. `2026-02-27` `TASK-017` `TODO -> DOING -> DONE`
    - Evidence: `OntologyCommand::ActionEnqueue` now delegates to `AxiomMe::enqueue_ontology_action`; schema parse/compile/validate+enqueue orchestration moved into core API with regression coverage.
18. `2026-02-27` `TASK-018` `TODO -> DOING -> DONE`
    - Evidence: `_service` suffix removed from top-level client module/file names (`indexing`, `relation`, `resource`, `runtime`, etc.) and module tree wiring updated with green workspace tests/audit.
19. `2026-02-27` `TASK-019` `TODO -> DOING -> DONE`
    - Evidence: queue status is now strongly typed via `QueueEventStatus` enum and migrated API/call sites; verification passed with `cargo check --workspace --all-targets`, `cargo test --workspace`, `cargo audit -q`.
20. `2026-02-27` `TASK-020` `TODO -> DOING -> DONE`
    - Evidence: `OntologyPressureTrigger` enum introduced with typed variants + string-contract serde; pressure/trend logic migrated to typed trigger vectors and workspace checks/tests/audit passed.
21. `2026-02-27` `TASK-014` `BLOCKED -> DOING -> BLOCKED`
    - Evidence delta: external viewer runtime probe passed via explicit override path (`AXIOMME_WEB_VIEWER_BIN=/Users/axient/repository/AxiomMe-web/target/debug/axiomme-webd` with `/api/fs/tree` probe `probe_rc=0`).
    - Remaining blocker owner/system: `release-owner` (final release decision not recorded).
    - Deterministic re-check evidence:
      - signoff entry in `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md` (`Final Release Decision`)
22. `2026-02-27` `TASK-021` `TODO -> DOING -> DONE`
    - Evidence: release signoff request packet created with required decision fields, reviewed evidence links, and deterministic completion steps (`docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`).
23. `2026-02-27` `TASK-022` `TODO -> DOING -> DONE`
    - Evidence: reviewer-reported type mismatch sites were rechecked and already aligned (`Node.uri: Arc<str>`, `uri_path_prefix_match` tests call with `&str`); local and workspace gates passed (`cargo check/test`, `cargo audit -q`).
24. `2026-02-27` `TASK-014` `BLOCKED -> DOING -> BLOCKED`
    - Evidence delta: technical mergeability gates revalidated after review comments (`cargo check --workspace --all-targets`, `cargo test --workspace`, `cargo audit -q` all pass).
    - Remaining blocker owner/system: `release-owner` (final release decision not recorded).
    - Deterministic re-check evidence:
      - signoff entry in `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md` (`Final Release Decision`)
      - signoff packet `docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`
25. `2026-02-27` `TASK-023` `TODO -> DOING -> DONE`
    - Evidence: release signoff status probe script added and executed (`scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md` => `rc=2`), with artifact output at `docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`.
26. `2026-02-27` `TASK-014` `BLOCKED -> DOING -> BLOCKED`
    - Evidence delta: automated probe confirms decision is still pending (`Overall: BLOCKED`, `Final Release Decision=PENDING`).
    - Remaining blocker owner/system: `release-owner` (final release decision not recorded).
    - Deterministic re-check evidence:
      - `scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`
      - `docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`
      - signoff entries in `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md`
27. `2026-02-27` `TASK-024` `TODO -> DOING -> DONE`
    - Evidence: added signoff apply command (`scripts/record_release_signoff.sh`) that writes both signoff docs and refreshes status artifact in one step.
28. `2026-02-27` `TASK-014` `BLOCKED -> DOING -> BLOCKED`
    - Evidence delta: `NX-022` now has a single deterministic execution command and required human input schema.
    - Remaining blocker owner/system: `release-owner` (explicit decision/name/date not provided yet).
    - Deterministic re-check evidence:
      - `scripts/record_release_signoff.sh --decision <GO|NO-GO> --name <name>`
      - `scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`
29. `2026-02-27` `TASK-025` `TODO -> DOING -> DONE`
    - Evidence: signoff model simplified to one final release decision (single owner/single command path).
30. `2026-02-27` `TASK-014` `BLOCKED -> DOING -> BLOCKED`
    - Evidence delta: blocker now depends only on `Final Release Decision` entry (`PENDING`) and no longer requires dual role fields.
    - Remaining blocker owner/system: `release-owner` (decision/name/date not provided yet).
    - Deterministic re-check evidence:
      - `scripts/record_release_signoff.sh --decision <GO|NO-GO> --name <name>`
      - `scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`
31. `2026-02-27` `TASK-014` `BLOCKED -> DOING -> DONE`
    - Evidence: final release decision recorded (`GO`, signer `aiden`) and probe turned `READY` (`scripts/release_signoff_status.sh ...` => `rc=0`).
32. `2026-02-27` `TASK-026` `TODO -> DOING -> DONE`
    - Evidence: claim-level cross-validation completed with direct code inspection + executable gates (`cargo check -p axiomme-core --lib`, `cargo test -p axiomme-core --lib`, `cargo audit -q`).
33. `2026-02-27` `TASK-027` `TODO -> DOING -> DONE`
    - Evidence: `ReconcileRunStatus` enum added and wired through reconcile run path; queue aggregation/dead-letter SQL now uses enum-derived status parameters (`crates/axiomme-core/src/models/reconcile.rs`, `crates/axiomme-core/src/client/queue_reconcile.rs`, `crates/axiomme-core/src/state/queue.rs`).
34. `2026-02-27` `TASK-028` `TODO -> DOING -> DONE`
    - Evidence: DB schema now enforces status `CHECK` domains for fresh DBs and migration adds normalization+validation guardrails for legacy rows with regression tests (`crates/axiomme-core/src/state/migration.rs`, `crates/axiomme-core/src/state/tests.rs`).
35. `2026-02-27` `TASK-029` `TODO -> DOING -> DONE`
    - Evidence: `InMemoryIndex::upsert` now builds text in one preallocated pass (`build_upsert_text`) and preallocates term-frequency map; compatibility tests confirm legacy text shape (`crates/axiomme-core/src/index.rs`).

## Next Action Mapping

- [NX-015] source:merged priority:P0 status:done action:Legacy FR-011 unblock path is superseded by explicit viewer override verification and dedicated human-signoff follow-up queue evidence: `NX-021` and `NX-022` entries in this file
- [NX-016] source:merged priority:P1 status:done action:Implement Phase 2 Task 2.1/2.2 by moving relation/tier domain logic from `fs.rs` to domain modules consumed by client/session layers evidence: `TASK-016` in this file + `crates/axiomme-core/src/relation_documents.rs` + `crates/axiomme-core/src/tier_documents.rs`
- [NX-017] source:merged priority:P1 status:done action:Implement Phase 3 Task 3.1 by moving ontology action enqueue orchestration from CLI handler into core `AxiomMe` method evidence: `TASK-017` in this file + `crates/axiomme-core/src/client/ontology.rs` + `crates/axiomme-cli/src/commands/mod.rs`
- [NX-018] source:merged priority:P1 status:done action:Implement Phase 3 Task 3.2 by renaming client `_service` modules to domain module names and updating module tree references evidence: `TASK-018` in this file + renamed `crates/axiomme-core/src/client/*.rs` modules + `crates/axiomme-core/src/client.rs`
- [NX-019] source:merged priority:P1 status:done action:Implement Phase 4 Task 4.1 by replacing raw queue status string API with strongly typed queue status enum across enqueue/fetch/status transitions evidence: `TASK-019` in this file + `crates/axiomme-core/src/models/queue.rs` + `crates/axiomme-core/src/state/queue.rs` + `cargo test --workspace`
- [NX-020] source:merged priority:P1 status:done action:Implement Phase 4 Task 4.2 by replacing ontology pressure trigger magic strings with `OntologyPressureTrigger` enum and explicit serialization at API boundary evidence: `TASK-020` in this file + `crates/axiomme-core/src/ontology/pressure.rs` + `crates/axiomme-core/src/ontology/mod.rs` + `cargo test --workspace`
- [NX-021] source:merged priority:P0 status:done action:Unblock FR-011 runtime dependency by validating web probe through explicit viewer override path and updating gate evidence evidence: `TASK-014` lifecycle entry `2026-02-27` + `docs/FEATURE_COMPLETENESS_UAT_GATE_2026-02-26.md` (`Platform/Tooling` signoff row) + `probe_rc=0`
- [NX-022] source:merged priority:P0 status:done action:Record final release decision (`GO` or `NO-GO`) to close final release gate evidence: `TASK-014` completion entry in this file + `scripts/record_release_signoff.sh --decision GO --name aiden` + `docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md` (`Overall: READY`)
- [NX-023] source:merged priority:P0 status:done action:Create a deterministic signoff request packet for release owners with explicit approval fields and completion steps evidence: `TASK-021` in this file + `docs/RELEASE_SIGNOFF_REQUEST_2026-02-27.md`
- [NX-024] source:merged priority:P0 status:done action:Validate reviewer-reported compile/type mismatch findings and confirm workspace mergeability gates evidence: `TASK-022` in this file + `cargo check/test` + `cargo audit -q`
- [NX-025] source:merged priority:P0 status:done action:Add automated release-signoff status probe and publish latest pending-role artifact for deterministic external follow-up evidence: `TASK-023` in this file + `scripts/release_signoff_status.sh` + `docs/RELEASE_SIGNOFF_STATUS_2026-02-27.md`
- [NX-026] source:merged priority:P0 status:done action:Add one-command signoff apply path for NX-022 with explicit decision inputs evidence: `TASK-024` in this file + `scripts/record_release_signoff.sh`
- [NX-027] source:merged priority:P0 status:done action:Simplify release signoff model to a single final decision evidence: `TASK-025` in this file + simplified gate/request/status scripts/docs
- [NX-028] source:ai priority:P1 status:done action:Implement typed queue status usage end-to-end in aggregate/reconcile paths (remove remaining hard-coded status literals in queue SQL/runtime) evidence: `TASK-027` in this file + `crates/axiomme-core/src/models/reconcile.rs` + `crates/axiomme-core/src/state/queue.rs`
- [NX-029] source:ai priority:P1 status:done action:Enforce queue/reconcile status domain at DB schema boundary to block invalid persisted values evidence: `TASK-028` in this file + `crates/axiomme-core/src/state/migration.rs` + `crates/axiomme-core/src/state/tests.rs`
- [NX-030] source:ai priority:P2 status:done action:Reduce write-path allocation pressure in `InMemoryIndex::upsert` while preserving output compatibility evidence: `TASK-029` in this file + `crates/axiomme-core/src/index.rs` + `cargo test -p axiomme-core --lib`
- Selected For Next: NONE
