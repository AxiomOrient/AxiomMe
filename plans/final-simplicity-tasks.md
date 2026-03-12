# AxiomSync Final Simplicity Task Table

> Archived note (2026-03-12): this task table was written for an older backend-abstraction track.
> The current repository direction is SQLite-only with `context.db` as the canonical store and no
> in-repo mobile FFI crate. Treat these rows as historical notes until they are explicitly replanned.

| TASK_ID | ACTION | DONE_WHEN | EVIDENCE_REQUIRED | DEPENDS_ON |
| --- | --- | --- | --- | --- |
| SIM-01 | `state/api.rs`에 concern별 subtrait와 composite `StateStore`, `StateStoreHandle`을 정의한다. | trait surface가 backend-agnostic 타입만 사용하고 orchestration에 필요한 method cluster만 포함하며 one-off SQLite admin API는 제외된다. | trait definition diff, method inventory checklist | - |
| SIM-02 | `AxiomSync`가 concrete `SqliteStateStore` 대신 `StateStoreHandle`을 보유하도록 바꾸고 `new_with_state` injection constructor를 추가한다. | `AxiomSync`가 runtime `StateStoreHandle`과 optional SQLite admin companion을 함께 보유하며, injection constructor로 임의 backend handle을 주입할 수 있다. | public API diff, compile evidence | SIM-01 |
| SIM-03 | `Session`도 `StateStoreHandle`을 보유하도록 바꾸고 기존 동작을 유지한다. | session 생성/clone/load 흐름이 concrete SQLite type 없이 유지된다. | session compile evidence, targeted tests | SIM-01, SIM-02 |
| SIM-04 | `SqliteStateStore`가 `StateStore` 계약을 구현하도록 정리하고, SQLite 고유 open/migrate/harden/admin 책임을 concrete 경로에 남긴다. | migration/open/permission hardening/one-off admin 경로는 SQLite concrete path에 있고 core trait에는 노출되지 않는다. | implementation diff, state tests | SIM-01, SIM-02, SIM-03 |
| SIM-05 | SQLite default backend factory와 optional SQLite admin companion 계약을 고정한다. | 기본 constructor는 SQLite를 열어 runtime `StateStoreHandle`과 SQLite admin companion을 함께 구성하고, `new_with_state` 경로의 SQLite-only admin 메서드는 명시적 unsupported/validation 에러로 실패한다. | constructor diff, admin-path integration test | SIM-04 |
| SIM-06 | `StateStore` conformance harness와 state injection smoke를 만들고 SQLite backend가 이를 통과하도록 한다. | SQLite가 store contract test suite를 통과하고, test-only backend double을 `new_with_state` 경로에 주입하는 smoke test가 1개 이상 존재한다. | conformance test output, state injection smoke test | SIM-04, SIM-05 |
| SIM-07 | `OperationContext`, `ActorKind`, `ApprovalRef` canonical 모델을 정의한다. | context 모델이 재사용 가능하고 serde round-trip 테스트가 통과한다. | model diff, round-trip unit test | - |
| SIM-08 | `RequestLogEntry`에 actor/work/approval 필드를 추가하고 기존 로그 포맷과 역호환되게 만든다. | old JSONL row를 새 모델로 읽을 수 있고 새 row에는 context 필드가 기록된다. | backward-compat test, request log snapshot | SIM-07 |
| SIM-09 | `client/request_log.rs`의 helper가 `OperationContext`를 입력받도록 바꾸고 legacy wrapper를 남긴다. | 새 request log helper 호출 경로가 context-aware helper를 사용한다. | grep evidence, logging helper tests | SIM-07, SIM-08 |
| SIM-10 | `AppConfig`에 `AuthzConfig`를 추가하고 `AXIOMSYNC_ENFORCE_AGENT_USER_MEMORY_BOUNDARY`를 파싱한다. | 설정이 기본 `false`로 동작하고 env로 명시적으로 켤 수 있다. | config unit test, env parsing test | SIM-07 |
| SIM-11 | session promotion 경로에 context를 주입할 수 있는 canonical surface를 추가한다. | `promote_session_memories_with_context`가 존재하고 session apply path까지 context가 전달된다. | public API diff, promotion integration test | SIM-03, SIM-07, SIM-09, SIM-10 |
| SIM-12 | 실제 memory write 경로에서 `agent -> user memory` 차단 정책을 적용한다. | enforcement off에서는 기존 성공, on에서는 `PermissionDenied`로 실패한다. | authz regression tests | SIM-10, SIM-11 |
| SIM-13 | session checkpoint/commit/archive mutation 경로에 `work_id`를 전달하도록 만든다. | 같은 작업에서 생성된 promotion/commit/request log가 동일 `work_id`를 가진다. | integration test with request logs | SIM-03, SIM-09, SIM-11 |
| SIM-14 | document/markdown save canonical surface에 context-aware variant를 추가한다. | save 성공/etag conflict/reindex rollback 경로 모두 context-aware API로 검증된다. | editor tests, request log assertion | SIM-02, SIM-07, SIM-09 |
| SIM-15 | resource/ontology/relation mutation entrypoints에 `_with_context` canonical surface를 추가한다. | 지속 상태를 바꾸는 핵심 mutation surface가 모두 context-aware 경로를 가진다. | API inventory checklist, targeted integration tests | SIM-02, SIM-07, SIM-09 |
| SIM-16 | outbox schema에 enqueue origin context를 저장하는 `context_json` 필드를 추가한다. | queue row가 optional `context_json`을 저장하고 legacy row도 계속 읽힌다. | SQLite migration test, schema diff | SIM-04, SIM-07 |
| SIM-17 | enqueue 경로에서 원본 context를 outbox에 기록하고 replay 경로에서 이를 복원한다. | enqueue -> replay -> request log -> dead-letter 흐름에서 `work_id`가 유지된다. | queue lifecycle integration test | SIM-09, SIM-15, SIM-16 |
| SIM-18 | `index_state` schema에 `size_bytes`를 추가하고 accessor (`upsert/get/list`)를 확장한다. | state layer가 `mtime + size`를 저장/조회하고 migration이 기존 DB를 안전하게 처리한다. | state migration tests | SIM-04 |
| SIM-19 | indexing pipeline이 file metadata size를 수집하고 state에 반영하도록 바꾼다. | 문서 색인 후 `index_state`에 올바른 size가 저장된다. | indexing tests, state assertions | SIM-18 |
| SIM-20 | runtime startup drift 판단과 reconcile 흐름을 `mtime + size` 기준으로 조정한다. | same-mtime changed-size 시 drift가 검출되고 재색인이 수행된다. | drift regression test | SIM-18, SIM-19 |
| SIM-21 | legacy `index_state` row에서 `size_bytes`가 없을 때의 fallback 정책을 구현한다. | 기존 DB를 가진 루트에서 첫 startup 후 안정적으로 steady state에 도달한다. | migration compatibility test | SIM-18, SIM-20 |
| SIM-22 | 운영 표면은 추가하지 말고 existing queue surface를 기준으로 표준 runbook를 문서화한다. | `queue status -> queue evidence -> queue daemon` 절차가 README/usage playbook에 일관되게 반영된다. | doc diff | SIM-17 |
| SIM-23 | `README.md`, `docs/API_CONTRACT.md`, `docs/ARCHITECTURE.md`, `docs/USAGE_PLAYBOOK.md`를 state boundary, mutation contract, drift contract에 맞춰 갱신한다. | 문서가 canonical `StateStore` seam, SQLite default, authz policy, drift contract를 모두 반영한다. | doc diff, manual review checklist | SIM-06, SIM-12, SIM-17, SIM-21, SIM-22 |
| SIM-24 | `docs/API_CONTRACT.md`의 episodic dependency revision을 실제 `crates/axiomsync/Cargo.toml`와 일치시킨다. | dependency contract mismatch가 제거된다. | doc diff, file comparison | - |
| SIM-25 | state seam, request log 역호환, context propagation, authz, drift migration 테스트를 보강한다. | 새 계약마다 최소 1개의 자동 테스트가 존재한다. | test list, passing test output | SIM-06, SIM-12, SIM-17, SIM-21 |
| SIM-26 | `scripts/quality_gates.sh` 기준 전체 품질 게이트를 다시 통과시킨다. | fmt, clippy, workspace tests, audit, notice gate가 모두 성공한다. | command output summary | SIM-23, SIM-24, SIM-25 |
| SIM-27 | final verification pass에서 계약-근거 매핑을 다시 확인한다. | 각 완료 기준에 대응하는 테스트/문서/로그 근거가 1개 이상 존재한다. | verification checklist | SIM-26 |

## Decision Gates

| GATE_NAME | CHECK | PASS_CONDITION | ON_FAIL |
| --- | --- | --- | --- |
| GATE-A State Store Boundary | trait가 orchestration seam까지만 캡슐화하는가 | `AxiomSync`와 `Session`은 normal runtime path에서 `StateStoreHandle`만 보고, SQLite lifecycle과 SQLite-only admin surface는 concrete/admin companion path에 남는다 | trait 범위를 줄이고 selector 추가를 금지한다 |
| GATE-B Context Freeze | context 필드 수가 최소인가 | `actor_kind`, `actor_id`, `work_id`, `approval` 외 필드가 추가되지 않는다 | 모델 축소 후 다시 동결한다 |
| GATE-C Queue Correlation | enqueue와 replay 사이에 `work_id`가 유지되는가 | queue lifecycle test에서 동일 `work_id`가 request log까지 이어진다 | `context_json` 구조를 더 줄이고 schema 증식을 막는다 |
| GATE-D Drift Migration | 기존 DB가 migration 후 안전한가 | legacy row startup test가 통과한다 | startup full reindex fallback을 강제한다 |
| GATE-E Contract Sync | 코드, 문서, dependency contract가 일치하는가 | README, API contract, architecture, Cargo dependency가 서로 동일하다 | 문서/계약부터 먼저 수정하고 코드 반영을 멈춘다 |
