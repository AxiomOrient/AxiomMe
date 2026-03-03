# AxiomMe OM v2 Big-Bang Renewal Plan

## 문서 메타
- Plan ID: `OMV2-BB-2026-03`
- Source: [`OM.md`](/Users/axient/repository/AxiomMe/OM.md), [`OM2.md`](/Users/axient/repository/AxiomMe/OM2.md)
- Strategy: `single-cutover big-bang`
- response_profile: `implementation_delta + clarify_question`

## 고정 의사결정
- D1. 이번 작업은 **big-bang 단일 전환**으로 수행한다.
- D2. **구버전 호환/병행 운영/dual-write를 하지 않는다.**
- D3. 정확성이 최우선이며, 성능/개발편의는 정확성을 훼손하지 않는 선에서만 최적화한다.
- D4. 필요 시 모듈 구조/네이밍/폴더 트리를 리뉴얼한다.
- D5. `episodic` 의존은 **정확한 git rev pin**으로 고정해 protocol drift를 차단한다.

## GOAL
- OM를 문자열 중심 v1 경로에서 완전히 제거하고, **구조화된 OM v2 단일 경로**로 전환한다.
- 검색/continuity/reflection/prompt contract 전 과정을 v2 프로토콜로 통일한다.

## DONE 정의
- v1 관련 핵심 경로(`active_observations` line-count merge, v1 hint reader, non-structured prompt path)가 코드에서 제거된다.
- 검색 힌트는 `snapshot -> priority compaction -> render`의 단일 경로로만 생성된다.
- reflection apply는 entry coverage(`covers_entry_ids`) 기반으로만 동작한다.
- observer/reflector 요청의 `request_json` 누락 경로가 0이 된다.
- Gate A/B/C를 모두 통과하고, 마지막에 destructive cleanup까지 완료된다.

## SCOPE
- 포함
- OM protocol v2 계약 고정
- observer/reflector 구조화 요청/응답
- canonical thread identity
- entry 기반 observation/reflection 모델
- snapshot 기반 search hint
- continuation 상태 분리
- deterministic fallback v2
- v1 코드/스키마 제거
- 제외
- OM 외 일반 검색 랭커 재설계
- UI 전면 개편
- 외부 검색엔진 변경

## CONSTRAINTS
- migration은 일회성 big-bang 전환으로 수행한다.
- 중간 호환 레이어(dual write/read, v1 fallback path) 금지.
- 실패 시 rollback은 코드/DB snapshot 복원으로 처리한다.
- 모든 주요 판단은 테스트 또는 계측 증적이 있어야 한다.

## AC (Acceptance Criteria)
- AC-01: `om_prompt_contract_mismatch_total == 0`.
- AC-02: `om_hint_high_priority_drop_total == 0`.
- AC-03: `om_reflection_apply_duplicates_total == 0`.
- AC-04: `request_json` 누락 호출 0건.
- AC-05: resource scope에서 canonical thread 선택 불일치 0건.
- AC-06: fallback 경로에서도 `current_task` 산출률 >= 95%.
- AC-07: Gate 테스트 스위트 전부 통과.

## Why-How-What
- Why
- OM.md/OM2.md 기준 병목은 검색엔진 교체가 아니라 OM 상태 모델/계약 불일치다.
- How
- OM v2를 단일 표준으로 강제하고 v1을 제거한다.
- What
- protocol, schema, runtime, search, reflection을 한 번에 재정렬한다.

## StoryBrand 7
- Character: 장기 작업 맥락을 정확히 유지해야 하는 사용자
- Problem: buffered 미가시성, thread 선택 오류, reflection 불안정
- Guide: OM v2 big-bang renewal
- Plan: 계약 고정 -> 데이터 모델 전환 -> 검색 전환 -> continuity/fallback 보강 -> v1 제거
- Call to Action: TASK-ID 순차 실행 후 Gate 승인
- Success: 안정적이고 설명 가능한 OM 검색/추론
- Failure: stale hint, 중복 reflection, 회귀 반복

## Message Fit
- 핵심 메시지: "호환보다 정확성. OM를 단일 v2 구조로 정리한다."

## Out-of-Scope
- 점진 롤아웃 설계
- v1 병행 운영
- 다중 백엔드 실험

## 리뉴얼 아키텍처 (Target)
- `crates/axiomme-core/src/om/v2/`
- `protocol.rs` (contract + version)
- `prompt_contract.rs` (observer/reflector request_json schema)
- `thread_identity.rs` (canonical thread resolver)
- `entries.rs` (observation/reflection entry model)
- `continuation.rs` (current_task/suggested_response reducer)
- `snapshot.rs` (search-visible snapshot builder)
- `hint_compaction.rs` (priority-aware deterministic renderer)
- `fallback.rs` (deterministic fallback engine)
- `migration.rs` (v1 -> v2 one-shot migration)

## 데이터 모델 (v2 only)
- `om_entries`
- `entry_id`, `scope_key`, `canonical_thread_id`, `priority`, `text`, `source_message_ids_json`, `origin_kind`, `created_at`, `superseded_by`
- `om_reflection_events`
- `event_id`, `scope_key`, `covers_entry_ids_json`, `reflection_entry_id`, `created_at`
- `om_continuation_state`
- `scope_key`, `canonical_thread_id`, `current_task`, `suggested_response`, `confidence`, `source_kind`, `updated_at`
- `om_protocol_meta`
- `protocol_version`, `episodic_rev`, `updated_at`

## 파일별 Patch Map
- Prompt/Contract
- [`crates/axiomme-core/src/session/om/observer/llm.rs`](/Users/axient/repository/AxiomMe/crates/axiomme-core/src/session/om/observer/llm.rs)
- [`crates/axiomme-core/src/client/mirror_outbox/reflector.rs`](/Users/axient/repository/AxiomMe/crates/axiomme-core/src/client/mirror_outbox/reflector.rs)
- Thread Identity
- [`crates/axiomme-core/src/session/om/observer/threading.rs`](/Users/axient/repository/AxiomMe/crates/axiomme-core/src/session/om/observer/threading.rs)
- [`crates/axiomme-core/src/client/search/mod.rs`](/Users/axient/repository/AxiomMe/crates/axiomme-core/src/client/search/mod.rs)
- State/Migration
- [`crates/axiomme-core/src/state/migration.rs`](/Users/axient/repository/AxiomMe/crates/axiomme-core/src/state/migration.rs)
- [`crates/axiomme-core/src/state/om.rs`](/Users/axient/repository/AxiomMe/crates/axiomme-core/src/state/om.rs)
- Search/Hint
- [`crates/axiomme-core/src/client/search/mod.rs`](/Users/axient/repository/AxiomMe/crates/axiomme-core/src/client/search/mod.rs)
- [`crates/axiomme-core/src/models/search.rs`](/Users/axient/repository/AxiomMe/crates/axiomme-core/src/models/search.rs)

## Execution Strategy (Big-Bang Single Wave)
1. Protocol Freeze
- `episodic` git rev pin
- prompt contract v2 schema 고정
2. Structural Cut
- canonical thread identity 도입
- entry/reflection/continuation v2 스키마 도입
- one-shot migration 구현
3. Runtime Cut
- search snapshot/hint v2 연결
- observer/reflector request_json 강제
- deterministic fallback v2 연결
4. Destructive Cleanup
- v1 상태/로직/테스트 제거
- deprecated naming/path 정리
5. Gate Certification
- Gate A/B/C 통합 통과 후 완료

## Decision Gates
### Gate A (Protocol/Schema)
- protocol version/rev pin 강제 확인
- prompt contract snapshot 통과
- one-shot migration dry-run 통과

### Gate B (Retrieval/Reflection)
- snapshot 기반 hint만 사용
- reflection duplicate/over-delete 0
- high-priority/current_task eviction 0

### Gate C (Continuity/Fallback)
- interval/async path에서도 continuity staleness 기준 통과
- deterministic fallback 정확도 기준 통과
- 전체 회귀 테스트 통과

## Verification Commands
```bash
cargo test -p axiomme-core om_bridge_contract -- --nocapture
cargo test -p axiomme-core session::om::tests -- --nocapture
cargo test -p axiomme-core queue_reconcile_lifecycle -- --nocapture
cargo test -p axiomme-core state::tests::om_reflection -- --nocapture
cargo test -p axiomme-core
cargo test
```

## Rollback Plan
- Rollback 방식은 호환 경로가 아니라 `pre-cut snapshot` 복원이다.
- 복원 대상
- DB 파일
- lockfile/Cargo pin
- OM 관련 모듈 트리
- 복원 트리거
- Gate A/B/C 중 하나라도 실패
- contract mismatch 또는 reflection duplicate 검출

## Risks
- R1: big-bang 범위로 인한 초기 충격
- R2: one-shot migration 실패 시 복구 비용
- R3: 구조 리뉴얼 중 네이밍/경계 누락

## Mitigations
- 단일 브랜치에서 Gate 전부 통과 전 머지 금지
- migration 리허설 + snapshot 복원 자동화
- contract snapshot 테스트를 merge blocker로 설정

## Clarify Resolution (고정)
- CQ-01 답: `episodic`는 git rev pin으로 고정한다.
- CQ-02 답: line-count 구경로는 병행 없이 제거한다.
- CQ-03 답: fallback suggested_response confidence threshold는 0.78로 시작한다.

## Progress Log (2026-03-03)
- Completed
- OMV2-001: `episodic` 의존을 git rev pin으로 고정하고 release gate 정책/테스트를 git+rev 계약으로 전환했다.
- OMV2-002: OM prompt contract v2를 `episodic@86b831e42186b8df663327ba6852c23a548685d1` 기준으로 고정하고 AxiomMe OM 경계(`crates/axiomme-core/src/om/mod.rs`)에서 재노출했다.
- OMV2-003: observer/reflector LLM 요청에 `request_json`(v2 contract) 주입을 강제했다.
- OMV2-005: one-shot migration(dry-run/apply) 경로를 상태 계층에 추가하고 무결성 검증 및 idempotency를 테스트로 고정했다.
- Verification
- `cargo fmt --all`
- `cargo test -p axiomme-core`
- `cargo test -p axiomme-core session::om::observer::llm::tests::observer_prompt_contract_json_contains_v2_contract_fields -- --exact`
- `cargo test -p axiomme-core client::mirror_outbox::reflector::tests::reflector_prompt_contract_json_contains_v2_contract_fields -- --exact`
- `cargo test -p axiomme-core state::tests::om_v2_migration_dry_run_reports_plan_without_writes -- --exact`
- `cargo test -p axiomme-core state::tests::om_v2_migration_apply_is_idempotent -- --exact`
- Remaining Critical Path
- OMV2-004 canonical thread identity 단일화
- OMV2-011 v1 코드/스키마 제거
- OMV2-012 Gate A 인증
