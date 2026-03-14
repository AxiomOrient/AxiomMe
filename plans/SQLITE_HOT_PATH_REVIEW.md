# SQLite Hot Path Review

`T03`의 목적은 SQLite 가 병목인지 감으로 말하지 않고, 실제 hot query 와 구조적 낭비를 확인한 뒤 가장 값싼 수정부터 넣는 것입니다.

## Reviewed Hot Paths

### 1. Outbox fetch
- location: `crates/axiomsync/src/state/queue.rs`
- query shape:
  - `status = ?`
  - `next_attempt_at <= ?`
  - `ORDER BY id ASC`
- change:
  - `COALESCE(next_attempt_at, created_at)` 제거
  - `idx_outbox_status_next_attempt_id` 추가

### 2. Timed-out processing recovery
- location: `crates/axiomsync/src/state/queue.rs`
- query shape:
  - `status = 'processing'`
  - `next_attempt_at <= ?`
- change:
  - `next_attempt_at` direct comparison 으로 단순화

### 3. Search restore
- location: `crates/axiomsync/src/state/search.rs`
- query shape:
  - `ORDER BY depth ASC, uri ASC`
- change:
  - `idx_search_docs_restore_order` 추가

## Structural Fixes Applied
- SQLite connection 에 `busy_timeout=5000ms` 설정
- outbox hot path 를 `next_attempt_at` direct predicate 로 정리
- outbox composite index 추가
- search restore order index 추가

## Evidence

가장 싼 증거는 state-level explain test 이다.

```bash
cargo test -q -p axiomsync state::tests::open_sets_busy_timeout_and_hot_path_indexes
```

이 테스트는 아래 두 조건을 실제로 확인한다.
- `PRAGMA busy_timeout = 5000`
- `EXPLAIN QUERY PLAN` 에서
  - `idx_outbox_status_next_attempt_id`
  - `idx_search_docs_restore_order`
  사용이 잡히는지

## Why This Closes T03
- hot query 목록이 명시되었다.
- 구조적 낭비인 `COALESCE(next_attempt_at, created_at)` 가 제거되었다.
- index 후보가 실제 schema 에 반영되었다.
- explain-based evidence 가 테스트에 고정되었다.
