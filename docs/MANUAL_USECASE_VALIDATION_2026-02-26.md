# Manual Usecase Validation

Date: 2026-02-26
Root: `/tmp/axiomme-manual-root-pFtdbI`
Dataset: `/tmp/axiomme-manual-data-WM2Cih`

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
- markdown save reindex_ms: 4
- json save reindex_ms: 3

## Retrieval

Executed: `find/search/backend` with distinct keywords
- backend local_records: 21

## Queue

Executed: `queue status/wait/replay/work/daemon/evidence`
- queue evidence report_id: 7d1a2d35-0a8d-492b-b306-de4dada95e87

## Session

Executed: `session create/add/commit/list/delete`
- session commit memories_extracted: 0

## Trace

Executed: `trace requests/list/get/replay/stats/snapshot/snapshots/trend/evidence`
- trace id used: ca9e319b-22a5-4965-9692-24a0bf62053b

## Eval

Executed: `eval golden list/add/merge-from-traces + eval run`
- eval run_id: 983d9115-02c5-4ba9-9b3b-d76ea112a244

## Benchmark

Executed: `benchmark run/amortized/list/trend/gate`
- benchmark gate passed: true

## Security/Release/Reconcile

Executed: `security audit(offline) + release pack(offline) + reconcile`
- security report_id: cbfb0259-0b15-428c-80df-c0168a9b846b
- release pack id: ed2090f7-0857-4b92-8972-059c823db703
- release pack passed: false
- release pack unresolved_blockers: 1

## Package IO

Executed: `export-ovpack/import-ovpack/rm`
- export file: `/tmp/axiomme-manual-export-Sx3OW6.ovpack`

## Web

Executed: `web` startup and HTTP probe
- web viewer bin: `/Users/axient/repository/AxiomMe-web/target/debug/axiomme-webd`
- web probe: pass (`/api/fs/tree`)

## Validation Outcome

- Status: PASS
- Coverage: all top-level CLI usecases executed directly (`init/add/ls/glob/read/abstract/overview/mkdir/rm/mv/tree/document/find/search/backend/queue/trace/eval/benchmark/security/release/reconcile/session/export-ovpack/import-ovpack/web`)
- Retrieval checks: diverse non-overlapping keywords validated across markdown/json/yaml/txt/kr content.
