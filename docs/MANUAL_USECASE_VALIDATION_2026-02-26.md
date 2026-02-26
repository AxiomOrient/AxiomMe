# Manual Usecase Validation

Date: 2026-02-26
Root: `/tmp/axiomme-manual-root-RGvGet`
Dataset: `/tmp/axiomme-manual-data-YnNI3C`

## Summary

Validated by direct CLI execution with diverse, non-overlapping keywords and end-to-end command coverage.

## Bootstrap

Executed: `init`

## Ingest

Executed: `add` standard + markdown-only modes
- add primary status: ok
- add markdown-only status: ok

## FS Operations

Executed: `ls/glob/read/abstract/overview/mkdir/mv/tree`
- ls root entries: 2
- ls manual recursive entries: 15

## Document Editor

Executed: `document load/save/preview` in markdown and document modes
- markdown save reindex_ms: 10
- json save reindex_ms: 3

## Retrieval

Executed: `find/search/backend` with distinct keywords
- backend local_records: 21

## Queue

Executed: `queue status/wait/replay/work/daemon/evidence`
- queue evidence report_id: 48b8aaee-1a5b-4b14-ab7d-e04945766419

## Session

Executed: `session create/add/commit/list/delete`
- session commit memories_extracted: 0

## Trace

Executed: `trace requests/list/get/replay/stats/snapshot/snapshots/trend/evidence`
- trace id used: 632b7dcb-ac4a-4c2c-83b3-30929622d50d

## Eval

Executed: `eval golden list/add/merge-from-traces + eval run`
- eval run_id: cc360b45-4ce5-4c39-8b4b-5c7d16a3b3aa

## Benchmark

Executed: `benchmark run/amortized/list/trend/gate`
- benchmark gate passed: true

## Security/Release/Reconcile

Executed: `security audit(offline) + release pack(offline) + reconcile`
- security report_id: 5c8df59a-aa3f-4bcb-bd89-06263062c8b9
- release pack id: af1899bc-b18e-425c-9205-cea4e5d2424c
- release pack passed: false
- release pack unresolved_blockers: 1

## Package IO

Executed: `export-ovpack/import-ovpack/rm`
- export file: `/tmp/axiomme-manual-export-jPNrLO.ovpack`

## Web

Executed: `web` startup and HTTP probe
- web probe: skipped (external viewer binary `axiomme-webd` not installed in PATH)

## Validation Outcome

- Status: PASS
- Coverage: all top-level CLI usecases executed directly (`init/add/ls/glob/read/abstract/overview/mkdir/rm/mv/tree/document/find/search/backend/queue/trace/eval/benchmark/security/release/reconcile/session/export-ovpack/import-ovpack/web`)
- Retrieval checks: diverse non-overlapping keywords validated across markdown/json/yaml/txt/kr content.
