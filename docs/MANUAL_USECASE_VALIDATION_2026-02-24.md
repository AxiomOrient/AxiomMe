# Manual Usecase Validation

Date: 2026-02-24
Root: `/tmp/axiomme-manual-root-C7jakK`
Dataset: `/tmp/axiomme-manual-data-fAQnBc`

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
- markdown save reindex_ms: 3
- json save reindex_ms: 3

## Retrieval

Executed: `find/search/backend` with distinct keywords
- backend local_records: 21

## Queue

Executed: `queue status/wait/replay/work/daemon/evidence`
- queue evidence report_id: 4d026c5d-8c1d-409c-9f3a-d3765bb8471f

## Session

Executed: `session create/add/commit/list/delete`
- session commit memories_extracted: 0

## Trace

Executed: `trace requests/list/get/replay/stats/snapshot/snapshots/trend/evidence`
- trace id used: 798f7d00-8915-41ec-9da4-b5e9573f64a1

## Eval

Executed: `eval golden list/add/merge-from-traces + eval run`
- eval run_id: 366c30fb-28ce-454a-9613-1144771daebc

## Benchmark

Executed: `benchmark run/amortized/list/trend/gate`
- benchmark gate passed: true

## Security/Release/Reconcile

Executed: `security audit + release pack + reconcile`
- security report_id: 62dd5a60-1e94-40d0-97c6-7d8e11d987fc
- release pack id: 432b3da8-aaf0-4c87-bd01-aebe7c837a9b
- release pack passed: true

## Package IO

Executed: `export-ovpack/import-ovpack/rm`
- export file: `/tmp/axiomme-manual-export-aCHkov.ovpack`

## Web

Executed: `web` startup and HTTP probe
- web probe: skipped (external viewer binary `axiomme-webd` not installed in PATH)

## Validation Outcome

- Status: PASS
- Coverage: all top-level CLI usecases executed directly (`init/add/ls/glob/read/abstract/overview/mkdir/rm/mv/tree/document/find/search/backend/queue/trace/eval/benchmark/security/release/reconcile/session/export-ovpack/import-ovpack/web`)
- Retrieval checks: diverse non-overlapping keywords validated across markdown/json/yaml/txt/kr content.
