# Tasks

| TASK-ID | Priority | Status | Description | Done Criteria | Evidence |
|---|---|---|---|---|---|
| SIM-001 | P0 | DONE | 계획/태스크 문서 생성 및 범위 고정 | `docs/IMPLEMENTATION-PLAN.md`/`docs/TASKS.md` 존재, scope/verification 정의 | `docs/IMPLEMENTATION-PLAN.md`, `docs/TASKS.md` |
| SIM-002 | P1 | DONE | search snapshot 거대 블록을 전용 모듈로 분리 | `search/mod.rs`에서 snapshot helper 제거, `search/snapshot.rs`로 이동, 테스트 import 유지 | `crates/axiomme-core/src/client/search/mod.rs`, `crates/axiomme-core/src/client/search/snapshot.rs` |
| SIM-003 | P1 | DONE | index upsert 계산 경로를 payload 단위로 분리 | `IndexDocumentPayload` 기반 upsert 경로 컴파일/테스트 통과 | `crates/axiomme-core/src/index.rs`, `cargo test -p axiomme-core index::tests::search_prioritizes_matching_doc -- --exact` |
| SIM-004 | P0 | DONE | 변경 검증 및 증거 고정 | narrow/full tests + clippy 모두 성공, 태스크 evidence 업데이트 | `cargo test -p axiomme-core --quiet`; `cargo clippy -p axiomme-core --all-targets -- -D warnings`; `cargo test -p axiomme-core client::search::tests::snapshot_visible_entry_ids_dedupes_same_chunk_source_keeping_first_entry -- --exact` |
| SIM-005 | P1 | DONE | search_with_request 오케스트레이션에서 힌트 해석/로그 사이드이펙트 분리 | 힌트 계산과 request-log 조립이 전용 helper로 추출되고, 동작/테스트 동일 | `crates/axiomme-core/src/client/search/mod.rs`; `cargo test -p axiomme-core --quiet`; `cargo clippy -p axiomme-core --all-targets -- -D warnings` |
| SIM-006 | P1 | DONE | search telemetry/query-plan 조립을 전용 모듈로 분리 | `search/telemetry.rs`에 telemetry 함수/로그 입력 모델 이동, `mod.rs` 오케스트레이션 집중, 테스트/클리피 통과 | `crates/axiomme-core/src/client/search/telemetry.rs`; `cargo test -p axiomme-core --quiet`; `cargo clippy -p axiomme-core --all-targets -- -D warnings` |
