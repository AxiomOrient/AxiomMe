# AxiomSync Execution Tracker

Last updated: 2026-03-14

## Mission
SQLite 를 유지한 채 코어 경계를 더 선명하게 만들고, 검색 계층은 SQLite 내부 강화와 성능 계측 순서로 진행한다.

## Working Set

| ID | Task | Status | Evidence Needed | Next Action |
|---|---|---|---|---|
| T01 | Ownership Map 작성 | done | [`docs/OWNERSHIP_MAP.md`](../docs/OWNERSHIP_MAP.md) 에 core / ops / vendored OM / shared model 경계 기록 | `T02` baseline automation 으로 이동 |
| T02 | 성능 기준선 측정 | done | [`plans/RUNTIME_BASELINE.md`](./RUNTIME_BASELINE.md) 와 `cargo run -p axiomsync --bin runtime_baseline -- ...` 로 JSON/markdown baseline 재현 가능 | `T03` SQLite hot path 와 연결 |
| T03 | SQLite access pattern 리뷰 | done | [`plans/SQLITE_HOT_PATH_REVIEW.md`](./SQLITE_HOT_PATH_REVIEW.md) 와 `state::tests::open_sets_busy_timeout_and_hot_path_indexes` 에 hot query / explain evidence 고정 | `T20` FTS5 prototype 로 이동 |
| T20 | SQLite FTS5 prototype | done | [`plans/FTS5_PROTOTYPE.md`](./FTS5_PROTOTYPE.md), `search_documents_fts_*` test, runtime top-hit comparison test 로 schema/rebuild/comparison 증거 확보 | 모든 working set 완료 |

## Current Recommendation
1. current working set `T01/T02/T03/T20` 은 완료됐다.
2. 다음 execution wave 는 `T04`, `T10`, `T11`, `T12`, `T21`, `T22` 중에서 다시 ledger 를 열어 선정한다.
3. 성능 변경은 `runtime_baseline`과 hot-path explain evidence 를 기준선으로 계속 비교한다.

## Turn Log
- 2026-03-14 Pass 1: `plans/README.md`를 추가해 네 개 전략 문서와 execution tracker 의 진입점을 만들었다.
- 2026-03-14 Pass 2: `docs/README.md`에서 stable docs 와 `plans/` planning surface 의 경계를 연결했다.
- 2026-03-14 Task T01: `docs/OWNERSHIP_MAP.md`를 추가하고 `docs/README.md`에 ownership entrypoint 를 연결했다.
- 2026-03-14 Task T02: `crates/axiomsync/src/bin/runtime_baseline.rs`와 `plans/RUNTIME_BASELINE.md`를 추가하고 `small` 시나리오 실행으로 JSON/markdown 출력 경로를 검증했다.
- 2026-03-14 Task T03: `busy_timeout`, outbox/search composite index, direct `next_attempt_at` predicate 를 추가하고 explain-based state test 와 `plans/SQLITE_HOT_PATH_REVIEW.md`로 근거를 고정했다.
- 2026-03-14 Task T20: `search_docs_fts` prototype schema와 triggers/rebuild 를 추가하고 FTS projection sync test 및 runtime top-hit comparison test 를 연결했다.
- 2026-03-14 Sync: working set 모든 row 를 `done`으로 맞추고 다음 wave 후보를 ledger 에 반영했다.

## Done Signal For This Turn
- `plans/README.md`가 네 개 전략 문서와 tracker 를 링크한다.
- `plans/TASKS.md`가 `T01 -> T02 -> T03 -> T20` 순서를 추적한다.
- `docs/README.md`가 planning 문서의 위치를 명시한다.
