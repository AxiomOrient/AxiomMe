# OM v2 Big-Bang Cutover Checklist

- Release ID: `OMV2-BB-2026-03-04`
- Owner: `AxiomMe Core`
- Status: `READY FOR SIGN-OFF`

## 1) Pre-Cutover Integrity
- [x] `episodic` git rev is pinned to `9f4c075bf26b81c8a81fbb6539c46ec20ea8a181`.
- [x] `Cargo.lock` source revision matches pinned rev.
- [x] Release gate tests for episodic manifest/lock contract are green.
- [x] OM prompt contract v2 request_json probes are green (observer/reflector).

## 2) Data/Schema Safety
- [x] one-shot migration dry-run passes (`om_v2_migration_dry_run_reports_plan_without_writes`).
- [x] one-shot migration apply idempotency test passes.
- [x] protocol metadata writes expected `protocol_version` + `episodic_rev`.

## 3) Runtime Cutover Correctness
- [x] reflection apply path uses `covers_entry_ids` contract.
- [x] legacy reflection seed fallback path is removed.
- [x] continuation read path does not fallback to record mirror fields.
- [x] snapshot v2 hint reader is active and noted in query plan.

## 4) Quality Gates
- [x] Gate A PASS (Protocol/Schema)
- [x] Gate B PASS (Retrieval/Reflection)
- [x] Gate C PASS (Continuity/Fallback)

## 5) Full Regression
- [x] `cargo test -p axiomme-core` pass
- [x] workspace `cargo test` pass
- [x] `/Users/axient/repository/episodic` `cargo test` pass

## 6) Rollback Readiness
- [x] rollback strategy documented as snapshot restore (no compatibility fallback).
- [x] restore targets defined: code rev, lockfile, state DB snapshot.
- [x] rollback trigger defined: any failed gate or contract mismatch.

## 7) Approval
- [ ] Technical owner sign-off
- [ ] Runtime owner sign-off
- [ ] Release approver sign-off

## Sign-off Record
- Approved At (KST): `TBD`
- Approved By: `TBD`
- Notes: `TBD`
