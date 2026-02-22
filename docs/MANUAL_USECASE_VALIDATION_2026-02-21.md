# Manual Usecase Validation

Date: 2026-02-21
Root: `/tmp/axiomme-manual-root-ve2AIx`
Dataset: `/tmp/axiomme-manual-data-j30KLy`

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
- backend local_records: 19

## Queue

Executed: `queue status/wait/replay/work/daemon/evidence`
- queue evidence report_id: 3990a5e9-3dce-416a-98b3-969b57429cf5

## Session

Executed: `session create/add/commit/list/delete`
- session commit memories_extracted: 0

## Trace

Executed: `trace requests/list/get/replay/stats/snapshot/snapshots/trend/evidence`
- trace id used: 41946a6a-8d89-4ff5-b8dd-53fd4089c094

## Eval

Executed: `eval golden list/add/merge-from-traces + eval run`
- eval run_id: 8e7d4a14-5db1-45f8-9a87-3b0e2779fe94

## Benchmark

Executed: `benchmark run/amortized/list/trend/gate`
- benchmark gate passed: true

## Security/Release/Reconcile

Executed: `security audit + release pack + reconcile`
- security report_id: 5b8e3288-ae96-47e9-bf9c-990e2abb80ef
- release pack id: 86fa93f8-1516-45b5-8a27-790190bf6da5
- release pack passed: true

## Package IO

Executed: `export-ovpack/import-ovpack/rm`
- export file: `/tmp/axiomme-manual-export-4WK1bZ.ovpack`

## Web

Executed: `web` startup and HTTP probe
- web probe: pass (`/api/fs/tree`)

## Validation Outcome

- Status: PASS
- Coverage: all top-level CLI usecases executed directly (`init/add/ls/glob/read/abstract/overview/mkdir/rm/mv/tree/document/find/search/backend/queue/trace/eval/benchmark/security/release/reconcile/session/export-ovpack/import-ovpack/web`)
- Retrieval checks: diverse non-overlapping keywords validated across markdown/json/yaml/txt/kr content.
