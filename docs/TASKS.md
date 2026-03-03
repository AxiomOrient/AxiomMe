# OM v2 Big-Bang Tasks

| TASK-ID | Priority | Status(TODO\|DOING\|DONE\|BLOCKED) | Description | Done Criteria | Evidence |
|---|---|---|---|---|---|
| OMV2-001 | P0 | DONE | `episodic` git rev pin 및 protocol version 고정 | Cargo/lock/문서에서 rev+version 단일값 고정 | 완료: `crates/axiomme-core/Cargo.toml`, `Cargo.lock`, `crates/axiomme-core/src/release_gate/{episodic_semver.rs,policy.rs,tests.rs}`, `cargo test -p axiomme-core` 통과 |
| OMV2-002 | P0 | DONE | Prompt Contract v2 스키마(observer/reflector) 정의 | request/response schema + snapshot fixture 통과 | 완료: `/Users/axient/repository/episodic@86b831e42186b8df663327ba6852c23a548685d1` (`src/prompt/contract.rs`), `crates/axiomme-core/src/om/mod.rs` re-export 반영 |
| OMV2-003 | P0 | DONE | observer/reflector `request_json` 강제 주입 | 누락 경로 0건 | 완료: `crates/axiomme-core/src/session/om/observer/llm.rs`, `crates/axiomme-core/src/client/mirror_outbox/reflector.rs`, `cargo test -p axiomme-core session::om::observer::llm::tests::observer_prompt_contract_json_contains_v2_contract_fields -- --exact`, `cargo test -p axiomme-core client::mirror_outbox::reflector::tests::reflector_prompt_contract_json_contains_v2_contract_fields -- --exact`, `cargo test -p axiomme-core` |
| OMV2-004 | P0 | TODO | canonical thread identity 리뉴얼 | grouping/selection/persistence 동일 canonical id 사용 | threading/search 회귀 테스트 |
| OMV2-005 | P0 | DONE | one-shot migration 설계/구현(v1->v2) | dry-run/실전 전환 성공, 무결성 검증 통과 | 완료: `state/migration.rs`(om_entries/om_continuation_state/om_protocol_meta + dry-run/apply API), `client/runtime.rs` API 노출, `state/tests.rs` 신규 2건(`om_v2_migration_dry_run_reports_plan_without_writes`, `om_v2_migration_apply_is_idempotent`), `cargo test -p axiomme-core` 통과 |
| OMV2-006 | P1 | TODO | entry 기반 observation/reflection 모델 도입 | line-count merge 제거, covers_entry_ids 기반 apply | state/reflection 테스트 |
| OMV2-007 | P1 | TODO | continuation state v2 분리 | current_task/suggested_response lifecycle 분리 동작 | continuation 테스트 |
| OMV2-008 | P1 | TODO | snapshot 기반 search hint v2 도입 | hint 생성 경로가 snapshot 단일 경로로 전환 | search 테스트/plan note |
| OMV2-009 | P1 | TODO | priority-aware hint compaction 적용 | high-priority/current_task eviction 0 | hint 회귀 테스트 |
| OMV2-010 | P1 | TODO | deterministic fallback v2 도입 | fallback에서도 continuity 산출 및 정확도 기준 통과 | fallback fixture 테스트 |
| OMV2-011 | P0 | TODO | v1 코드/스키마/테스트 제거(Destructive Cleanup) | v1 경로 참조 제거 및 빌드/테스트 통과 | 코드 검색 + 테스트 결과 |
| OMV2-012 | P0 | TODO | Gate A 인증 | OMV2-001~005 완료 + gate test pass | 명령 로그 |
| OMV2-013 | P1 | TODO | Gate B 인증 | OMV2-006~009 완료 + gate test pass | 명령 로그 |
| OMV2-014 | P1 | TODO | Gate C 인증 | OMV2-010~011 완료 + gate test pass | 명령 로그 |
| OMV2-015 | P0 | TODO | Big-Bang 릴리스 승인 문서화 | 전환/복원 체크리스트 완료 | release note + checklist |

## Execution Order
- Wave-1: OMV2-001, OMV2-002, OMV2-003, OMV2-004, OMV2-005
- Wave-2: OMV2-006, OMV2-007, OMV2-008, OMV2-009, OMV2-010
- Wave-3: OMV2-011, OMV2-012, OMV2-013, OMV2-014, OMV2-015

## Transition Rules
- 상태 전이: `TODO -> DOING -> DONE` 또는 `TODO/DOING -> BLOCKED`
- `DONE` 갱신 시 같은 행의 `Evidence`를 커맨드/파일 기준으로 즉시 갱신
- `BLOCKED`는 unblock 조건을 같은 행 `Evidence`에 명시

## Quality Gates
- Gate A
- protocol rev/version 고정
- prompt contract snapshot pass
- migration dry-run pass
- Gate B
- reflection duplicate 0
- hint high-priority drop 0
- snapshot-only hint path 사용
- Gate C
- fallback continuity 기준 충족
- 전체 테스트 pass

## Next Actions
- [NX-001] source:task priority:P0 status:todo action:OMV2-004 canonical thread identity를 observer/search/state 전 경로에 단일화 evidence:docs/IMPLEMENTATION-PLAN.md
- [NX-002] source:task priority:P0 status:todo action:OMV2-011 v1 코드/스키마/테스트 제거 전수 정리 evidence:docs/IMPLEMENTATION-PLAN.md
- [NX-003] source:task priority:P0 status:todo action:OMV2-012 Gate A 인증(OMV2-001~005 완료 스냅샷 기준) evidence:docs/IMPLEMENTATION-PLAN.md
- Selected For Next: NX-001, NX-002, NX-003

## NX -> TASK-ID Mapping
- NX-001 -> OMV2-004 (TODO)
- NX-002 -> OMV2-011 (TODO)
- NX-003 -> OMV2-012 (TODO)
