## 결론

장기안으로는 8개 항목을 각각 따로 땜질하는 방식보다, **OM v2 프로그램**으로 묶어서 구현하는 것이 맞습니다. 핵심은 `episodic`를 **버전 관리되는 OM 프로토콜 / pure transform 레이어**로 고정하고, AxiomMe는 **상태 저장·검색 가시화·런타임 정책 적용 레이어**로 정리하는 것입니다. 현재 코드 기준으로는 AxiomMe가 `episodic` 0.1.0에 의존하고 `episodic`의 pure contract를 대량 re-export하고 있으며, 검색 힌트는 `record.active_observations`와 `suggested_response` 중심으로 구성됩니다. 반면 상태 계층에는 별도의 `om_observation_chunks` 저장/조회 경로가 있고, single-thread observer 프롬프트는 `request_json: None`으로 호출되며, multi-thread observer는 `source_thread_id.or(source_session_id)`로 그룹을 만들지만 최종 선호 텍스트는 `current_session_id`로 고릅니다. `episodic` 쪽은 interval-triggered이면서 threshold 미도달이면 continuation hints를 건너뛰고, reflection은 line-count 기반 merge를 사용하며, 프롬프트 규약은 이미 High/Medium/Low priority와 `<current-task>`, `<suggested-response>`를 명시합니다. 이 조합은 “OM 정보가 있음에도 읽기 모델과 상태 모델이 분리되어 있고, 계약이 versioned protocol로 굳어 있지 않다”는 뜻입니다. ([GitHub][1])

권장 구현 순서는 다음입니다.

1. **OM-08 계약/버전 거버넌스**
2. **OM-03 구조화 프롬프트 계약**
3. **OM-04 canonical thread identity**
4. **OM-06 entry-based reflection/observation model**
5. **OM-01 search-visible snapshot**
6. **OM-07 priority-aware hint compaction**
7. **OM-02 continuation state 분리**
8. **OM-05 deterministic fallback v2**

이 순서가 좋은 이유는, 1~4가 **프로토콜·식별자·데이터 모델**을 고정하고, 5~7이 **읽기 품질과 검색 사용성**을 올리며, 8이 **LLM 실패 시 품질 하한선**을 올리기 때문입니다. 아래 계획서는 이 순서를 기준으로 작성했습니다.

---

## 근거

AxiomMe는 `episodic`를 runtime boundary 바깥의 pure OM 코어로 취급하고 있으며, `ActivationBoundary`, `OmObserverRequest`, `OmObserverPromptInput`, `build_observer_user_prompt`, `should_skip_observer_continuation_hints`, `merge_buffered_reflection` 같은 타입/함수를 그대로 re-export합니다. 즉 장기안은 AxiomMe 내부 휴리스틱을 늘리는 것이 아니라, **`episodic`의 계약을 먼저 키우고 AxiomMe가 그것을 채택하는 방식**이 자연스럽습니다. ([GitHub][2])

또한 현재 search read-path는 `build_om_hint_state_from_record()`에서 `record.active_observations`와 `suggested_response` 계열만 사용합니다. 반면 상태 저장소는 `om_observation_chunks`를 append/list/clear 하는 별도 저장 모델을 갖고 있습니다. 코드 경로상으로 보면, buffered chunk는 별도 materialization 단계 없이는 검색 힌트에 직접 반영되지 않는 것으로 해석하는 것이 타당합니다. 이는 검색 freshness와 task continuity 지연의 원인으로 볼 수 있습니다. ([GitHub][3])

---

# OM v2 구현 계획서

## 공통 설계 원칙

### 1) `episodic`는 프로토콜, AxiomMe는 어댑터

* `episodic`

  * 타입 정의
  * pure transform
  * schema/version
  * deterministic rules
* AxiomMe

  * SQLite schema
  * read/write orchestration
  * search integration
  * rollout/telemetry

### 2) dual-write → dual-read → cutover

* 기존 `active_observations: String` 경로는 즉시 삭제하지 않습니다.
* 새 entry model / continuation state / thread identity를 먼저 병행 기록합니다.
* 읽기 경로를 새 모델로 바꾼 뒤, 마지막 단계에서 기존 필드를 축소합니다.

### 3) 상태와 렌더를 분리

* 저장은 구조화
* 표시/search hint는 렌더 결과
* reflection은 텍스트 줄 수가 아니라 **구조화된 항목 집합**에 적용

### 4) 결정성 우선

* 같은 입력이면 같은 hint
* 같은 entry set이면 같은 reflection merge
* 같은 prompt contract면 같은 request_json

---

## OM-01: Buffered observation을 search-visible snapshot으로 승격

현재 검색 힌트는 `active_observations` + `suggested_response`만 사용하고, buffered observation chunk는 별도 상태 테이블에 있습니다. 장기적으로는 “활성화된 것만 보이는 모델”이 아니라 **검색 시점에 materialize된 OM snapshot**을 읽게 바꿔야 합니다. ([GitHub][3])

### 목표

* buffered observation이 activation 이전에도 **정책적으로 허용된 범위 내에서** 검색 가시성에 반영되게 함
* search read-path가 raw record string이 아니라 **versioned snapshot**을 읽게 함

### 최종 상태

* `episodic`에 `OmSearchVisibleSnapshotV2` 추가
* AxiomMe는 scope_key 기준으로

  * activated entries
  * buffered entries tail
  * continuation state
  * preferred thread overlay
    를 합쳐 snapshot을 생성
* search는 `bounded_om_hint_from_record()` 대신 snapshot renderer를 사용

### 설계 산출물

* `OmSearchVisibleSnapshotV2`

  * `scope_key`
  * `activated_entry_ids`
  * `buffered_entry_ids`
  * `current_task`
  * `suggested_response`
  * `rendered_hint`
  * `materialized_at`
  * `snapshot_version`
* `SearchVisiblePolicy`

  * buffered 포함 여부
  * max buffered tail
  * preferred thread 규칙
  * privacy/redaction 규칙

### 구현 태스크

1. **프로토콜 정의**

   * `episodic`에 `OmSearchVisibleSnapshotV2` 타입 추가
   * `materialize_search_visible_snapshot()` pure transform 추가

2. **AxiomMe read-model 추가**

   * `scope_key -> snapshot` 생성 함수 추가
   * source: OM record, buffered observation chunks, thread state, continuation state

3. **캐시/무효화 설계**

   * snapshot cache key

     * `generation_count`
     * last buffered seq
     * thread state updated_at
     * continuation version
   * stale snapshot invalidation 규칙 정의

4. **search 통합**

   * `build_om_hint_state_from_record()`를 `build_om_hint_state_from_snapshot()`로 교체
   * query-plan note에 snapshot source range 기록

5. **관측성**

   * metrics

     * `om_snapshot_build_ms`
     * `om_snapshot_buffered_entry_count`
     * `om_snapshot_freshness_lag_ms`

6. **롤아웃**

   * feature flag: `AXIOMME_OM_HINT_READER=v2`
   * dual-read A/B 비교 로그 추가

### 종료 조건

* buffered entry가 activation 전에도 snapshot에 반영 가능
* 같은 source set이면 snapshot 문자열이 byte-identical
* search path가 raw `active_observations` 직접 참조를 중단

### 의존성

* OM-04 thread identity
* OM-06 entry model
* OM-07 hint compactor

---

## OM-02: Continuation state를 observation activation과 분리

`episodic`는 interval-triggered + threshold 미도달 상황에서 continuation hint를 건너뛰도록 설계돼 있습니다. 현재 구조를 그대로 두면 `current_task`와 `suggested_response`가 observation activation 주기에 종속됩니다. 장기적으로는 continuation을 **독립된 상태 모델**로 분리해야 합니다. ([GitHub][4])

### 목표

* `current_task`와 `suggested_response`를 observation text와 별도 lifecycle로 관리
* async/interval path에서도 task continuity가 stale하지 않게 유지

### 최종 상태

* `OmContinuationStateV2` 도입
* source-of-truth는 observation response가 아니라 **continuation reducer**
* update source:

  * observer sync path
  * observer async path
  * deterministic fallback
  * reflector
  * explicit user-task change detector

### 설계 산출물

* `OmContinuationStateV2`

  * `scope_key`
  * `thread_id`
  * `current_task`
  * `suggested_response`
  * `confidence`
  * `source_kind`
  * `source_message_ids`
  * `updated_at`
  * `staleness_budget_ms`
* `ContinuationPolicyV2`

  * async observer는 current_task만 허용
  * suggested_response는 더 보수적
  * reflector는 only-improve rule
  * conflict resolution 우선순위

### 구현 태스크

1. **pure reducer 정의**

   * `resolve_continuation_update(previous, candidate, policy)`

2. **candidate model 정의**

   * LLM observer output
   * deterministic extractor output
   * reflector output
   * explicit user task change signal

3. **AxiomMe 저장 모델 추가**

   * scope/thread 기준 continuation state 저장
   * source metadata 포함

4. **read-path 변경**

   * search / OM UI / debugging path가 continuation state를 읽도록 변경

5. **staleness 관리**

   * 오래된 suggested_response 자동 만료
   * current_task는 TTL과 explicit close signal 지원

6. **충돌 정책**

   * 같은 scope에 여러 thread가 있을 때
   * preferred thread 우선, 그 외 fallback

7. **관측성**

   * `om_continuation_updates_total{source_kind=...}`
   * `om_continuation_stale_reads_total`

### 종료 조건

* async observe만 반복되어도 `current_task`가 stale하지 않음
* suggested_response는 빈번히 흔들리지 않음
* continuation read-path가 active_observations string에 의존하지 않음

### 의존성

* OM-03 prompt contract
* OM-04 thread identity
* OM-05 deterministic fallback v2

---

## OM-03: Observer/Reflector를 구조화 프롬프트 계약으로 승격

현재 single-thread observer LLM 호출은 `OmObserverPromptInput { request_json: None, ... }`로 user prompt를 구성합니다. multi-thread path 역시 active observations, threads, skip flag 중심으로 프롬프트를 만듭니다. 장기적으로는 prompt를 “문자열 템플릿”이 아니라 **버전이 있는 JSON 계약 + 렌더러**로 다뤄야 합니다. ([GitHub][5])

### 목표

* observer/reflector 입출력을 schema-first로 정규화
* prompt 변경이 제품 코드 전반에 암묵적으로 퍼지지 않게 고정

### 최종 상태

* `OmPromptContractV1`

  * observer single-thread
  * observer multi-thread
  * reflector
* AxiomMe는 항상 `request_json`을 넘김
* prompt text는 contract renderer 결과
* response parser는 contract version을 기준으로 검증

### 설계 산출물

* `OmPromptContractHeader`

  * `contract_name`
  * `contract_version`
  * `scope`
  * `scope_key`
* `OmObserverContractV1`
* `OmReflectorContractV1`
* `OmResponseContractV1`

  * required tags
  * optional fields
  * size limits
  * normalization rules

### 구현 태스크

1. **schema 명세 작성**

   * JSON schema 문서
   * XML/JSON 허용 응답 형식 명세
   * contract version 정책

2. **`episodic` prompt builder 리팩터**

   * builder 입력을 string fragments에서 contract DTO로 전환
   * single/multi-thread 공통 렌더 기반 통합

3. **AxiomMe 호출부 변경**

   * single-thread observer: `request_json` 항상 채움
   * multi-thread observer: thread list, known ids, preferred thread, limits 포함
   * reflector도 동일 패턴 적용

4. **parser 강화**

   * contract version mismatch error
   * missing required field diagnostics
   * response normalization trace

5. **golden 테스트**

   * prompt snapshot
   * response parse snapshot
   * backward compatibility fixtures

6. **문서화**

   * `PROMPT_CONTRACT.md`
   * 릴리스마다 변경점 기록

### 종료 조건

* observer/reflector LLM 요청의 100%에 `request_json` 존재
* prompt 변경은 contract version bump 없이 merge 불가
* parser 오류가 “어떤 계약이 깨졌는지”를 출력

### 의존성

* OM-08 버전 거버넌스

---

## OM-04: Canonical thread identity 도입

현재 multi-thread observer는 resource scope에서 `source_thread_id.or(source_session_id)`로 그룹을 만들고, 최종 `current_task`/`suggested_response` 선택은 `current_session_id`를 preferred id로 사용합니다. search read-path도 resource scope에서 thread preference를 `scope_binding.thread_id.unwrap_or(session_id)`로 잡습니다. 장기적으로는 **thread 식별자 해석 규칙을 한 곳으로 모아 canonical identity를 만들어야** 합니다. ([GitHub][6])

### 목표

* thread grouping, thread state 저장, search preferred thread 선택을 동일한 identity 규칙으로 통합
* session id와 thread id가 섞여도 오동작하지 않게 함

### 최종 상태

* `OmThreadRefV2`

  * `canonical_thread_id`
  * `scope`
  * `scope_key`
  * `origin_thread_id`
  * `origin_session_id`
  * `resource_id`
* 모든 grouping/selection/persistence가 `canonical_thread_id` 사용

### 설계 산출물

* `resolve_canonical_thread_ref(...)`
* `PreferredThreadResolutionPolicy`
* thread equivalence / alias 규칙

### 구현 태스크

1. **식별자 정책 명세**

   * resource/session/thread scope별 canonicalization 규칙
   * fallback 순서 정의
   * empty/null/id mismatch 처리 정의

2. **`episodic` 또는 boundary helper 정의**

   * pure resolver가 받을 최소 입력 형식 합의

3. **AxiomMe write-path 교체**

   * observer batching/grouping
   * thread state 저장
   * search preferred thread selection
   * continuation state update

4. **기존 데이터 마이그레이션**

   * old thread_id → canonical_thread_id backfill
   * alias map 유지 여부 결정

5. **충돌/분기 처리**

   * 하나의 session에 여러 thread가 있을 때
   * resource scope에서 source ids가 누락된 경우

6. **디버깅 표면**

   * query-plan / OM debug 출력에 origin vs canonical 표시

### 종료 조건

* grouping과 preferred selection이 같은 id 체계를 사용
* session id/thread id 혼합 fixture에서 current_task 선택이 안정적
* thread state가 canonical id 기준으로 유일하게 저장됨

### 의존성

* OM-08
* OM-03

---

## OM-05: Deterministic fallback을 정식 엔진으로 승격

현재 deterministic observer response는 observation text만 합성하고 `current_task`와 `suggested_response`를 `None`으로 둡니다. 이 구조는 strict mode 실패, model off, local model 오류 시 OM의 품질 하한선을 너무 낮춥니다. 장기적으로는 deterministic path를 **비상 탈출구가 아니라 정식 1등급 엔진**으로 설계해야 합니다. ([GitHub][7])

### 목표

* LLM이 없어도 OM가 최소 수준의 continuity를 유지
* deterministic path가 `current_task`, `suggested_response`, evidence를 산출

### 최종 상태

* `DeterministicObservationEngineV2`
* rule-based but versioned
* output:

  * observations
  * current_task
  * suggested_response
  * evidence spans / source ids
  * confidence

### 설계 산출물

* `DeterministicCandidate`
* `DeterministicEvidence`
* `DeterministicContinuationHints`
* `DeterministicPolicyV2`

### 구현 태스크

1. **규칙 셋 정의**

   * user imperative / task verbs
   * tool error signals
   * assistant blocked state
   * identifier extraction
   * file/function/config key extraction

2. **evidence model**

   * 어떤 메시지/문장에서 task를 뽑았는지 저장
   * confidence와 ambiguity flags 기록

3. **`episodic` pure transform 구현**

   * `infer_deterministic_observer_response()`
   * `infer_deterministic_continuation()`

4. **AxiomMe fallback 경로 교체**

   * strict=false fallback
   * model disabled path
   * parse failure path

5. **평가 세트**

   * user question
   * tool error
   * multi-turn task continuation
   * file/config heavy dialogues

6. **안전장치**

   * low-confidence suggested_response는 suppress
   * hallucination-prone fields는 evidence 없으면 비움

### 종료 조건

* deterministic path에서 `current_task` 공란 비율이 크게 줄어듦
* evidence 없는 invented identifier가 생성되지 않음
* strict failure 시 search/OM UX가 급락하지 않음

### 의존성

* OM-02 continuation state
* OM-03 prompt/output contract
* OM-04 canonical thread identity

---

## OM-06: Reflection을 line-count 모델에서 entry-based compaction 모델로 전환

`episodic`의 reflection draft는 source line 수를 세고, AxiomMe는 `reflected_observation_line_count`를 사용해 active observations 앞부분을 대체합니다. 이 구조는 모델이 line wrap을 바꾸거나 문장을 합치면 중복/과삭제가 발생하기 쉽습니다. 장기적으로는 reflection을 **entry ID 기반 compaction**으로 바꿔야 합니다. ([GitHub][8])

### 목표

* reflection apply가 줄 수가 아니라 **observation entry 집합**에 대해 작동
* merge가 idempotent하고 추적 가능해야 함

### 최종 상태

* `OmObservationEntryV2`
* `OmReflectionResponseV2`

  * `covers_entry_ids`
  * `reflection_text`
  * optional continuation
* active observation text는 entry set에서 렌더링된 결과물일 뿐, 원본이 아님

### 설계 산출물

* `OmObservationEntryV2`

  * `entry_id`
  * `scope_key`
  * `thread_id`
  * `priority`
  * `text`
  * `source_message_ids`
  * `origin_kind`
  * `created_at`
  * `superseded_by`
* `ReflectionCoverageSet`
* `ObservationRenderPolicy`

### 구현 태스크

1. **entry model 명세**

   * observation/chunk/summary/reflection entry 구분
   * supersede / cover / retain semantics 정의

2. **SQLite schema 추가**

   * `om_observation_entries`
   * `om_reflection_events`
   * 필요 시 `om_entry_edges`

3. **dual-write**

   * 기존 active_observations string 유지
   * 동시에 entry log 기록
   * active_observations는 render result로 생성

4. **backfill**

   * 기존 active lines를 synthetic entry로 변환
   * synthetic source metadata 부여

5. **reflector contract 개편**

   * response에 `covers_entry_ids` 포함
   * parser/validator 추가

6. **apply logic 교체**

   * covered entries mark superseded
   * reflection entry append
   * render active view 재생성

7. **GC/compaction**

   * superseded entry archive 정책
   * render cost 상한 유지

### 종료 조건

* reflection merge가 line wrap 변화에 영향받지 않음
* covered/uncovered entry가 추적 가능
* replay 시 같은 entry graph에서 같은 active view 생성

### 의존성

* OM-03 structured prompt contract
* OM-04 canonical thread identity
* OM-08 schema/version governance

---

## OM-07: Priority-aware, deterministic hint compaction

`episodic`의 prompt 시스템은 High/Medium/Low priority와 `<current-task>`, `<suggested-response>`를 명시하지만, 현재 search hint는 본질적으로 bounded text + suggested_response 결합 모델입니다. 장기적으로는 hint compaction이 **entry priority / freshness / task continuity / thread preference**를 반영하는 전용 렌더러가 되어야 합니다. ([GitHub][9])

### 목표

* 중요한 관찰이 최근 tail에 밀려 사라지지 않게 함
* search hint를 짧게 유지하면서도 continuity를 보존

### 최종 상태

* `OmHintPolicyV2`
* `render_search_hint(snapshot, policy)` pure transform
* hint는 entry selection 결과 + continuation state로 생성

### 설계 산출물

* selection score 축

  * priority
  * freshness
  * task alignment
  * thread preference
  * novelty/diversity
* deterministic tie-break

  * priority desc
  * created_at desc
  * entry_id asc

### 구현 태스크

1. **selection policy 명세**

   * current_task/suggested_response는 reservation slot
   * High priority entry 보장 슬롯
   * buffered tail quota
   * max lines / max chars

2. **`episodic` renderer 구현**

   * query-agnostic deterministic renderer
   * optional future query-aware overlay는 분리 설계

3. **AxiomMe search 통합**

   * snapshot → hint renderer
   * query-plan note에 selected entry ids 기록

4. **explainability**

   * debug output에 “왜 이 entry가 선택되었는지” 표기

5. **회귀 테스트**

   * High priority survival
   * current_task reservation
   * deterministic order
   * same input same output

### 종료 조건

* High priority entry가 tail noise에 의해 탈락하지 않음
* current_task는 항상 남음
* identical snapshot에서 hint 문자열이 항상 동일

### 의존성

* OM-01 search-visible snapshot
* OM-02 continuation state
* OM-06 entry model

---

## OM-08: `episodic`를 versioned protocol로 운영

AxiomMe는 현재 `episodic = "0.1.0"`에 의존하고 있고, `episodic` 저장소의 현재 버전은 `0.1.1`입니다. 동시에 AxiomMe는 `episodic`의 타입과 transform을 runtime boundary에서 직접 re-export합니다. 이 구조에서는 `episodic`가 사실상 **OM 프로토콜 저장소**이므로, 버전/호환성/릴리스 절차를 명시적으로 가져가야 합니다. ([GitHub][1])

### 목표

* `episodic` 변경이 AxiomMe에 암묵적으로 전파되지 않게 함
* 프로토콜/스키마 변경이 CI에서 잡히게 함

### 최종 상태

* `OM_PROTOCOL_VERSION`
* schema compatibility matrix
* consumer contract test suite
* co-development pinning strategy 명문화

### 설계 산출물

* `PROTOCOL.md`
* `COMPATIBILITY.md`
* `RELEASE_CHECKLIST.md`
* `fixtures/contracts/*`

### 구현 태스크

1. **버전 정책 정의**

   * semver + protocol version 분리
   * breaking/non-breaking 기준 정의

2. **개발 모드 pinning**

   * co-dev 시 `[patch.crates-io]` 또는 git rev pin
   * release 전 crates.io semver 재동기화

3. **호환성 테스트**

   * prompt contract snapshot
   * response parse fixtures
   * reflection apply fixtures
   * deterministic fallback fixtures

4. **CI 게이트**

   * `episodic` 변경 시 AxiomMe consumer tests mandatory
   * protocol version bump without changelog 금지

5. **문서화**

   * 변경 전파 규칙
   * upgrade guide
   * deprecation window 규칙

### 종료 조건

* `episodic` breaking change가 AxiomMe CI에서 즉시 검출
* protocol version 없는 contract 변경이 merge 불가
* co-development 시 version drift가 통제됨

### 의존성

* 없음. 가장 먼저 시작

---

# 프로그램 단위 공통 태스크

## A. 테스트 자산 먼저 만들기

이 8개 항목은 전부 상태/계약/검색 가시화가 얽혀 있으므로, 구현 전에 **고정 fixture 세트**를 먼저 만드는 것이 좋습니다.

### 필수 fixture

* session scope, 단일 thread
* resource scope, 다중 thread / 다중 session 혼합
* async observe만 여러 번 발생하는 케이스
* LLM parse 실패 후 deterministic fallback
* reflection이 line merge/split를 유발하는 케이스
* high-priority observation이 tail에 밀리는 케이스

### 필수 골든 산출물

* observer prompt JSON
* observer rendered prompt text
* reflector prompt JSON
* search-visible snapshot JSON
* rendered search hint
* reflection apply result
* deterministic fallback output

---

## B. 마이그레이션 전략

### 원칙

* destructive migration 금지
* dual-write 유지
* old read-path는 feature flag 뒤에 남김

### 단계

1. schema add
2. dual-write on
3. background/backfill
4. shadow-read compare
5. read cutover
6. old field deprecate
7. final cleanup

### 필요한 운영 도구

* `axiomme om migrate --backfill-entries`
* `axiomme om audit --compare-v1-v2`
* `axiomme om debug --scope-key ...`

---

## C. 관측성 / 지표

### 꼭 넣어야 할 메트릭

* `om_snapshot_build_ms`
* `om_snapshot_stale_reads_total`
* `om_continuation_staleness_ms`
* `om_thread_identity_conflicts_total`
* `om_prompt_contract_mismatch_total`
* `om_reflection_apply_duplicates_total`
* `om_hint_high_priority_drop_total`
* `om_fallback_used_total`
* `om_fallback_low_confidence_total`

### 꼭 남겨야 할 로그 필드

* `scope_key`
* `canonical_thread_id`
* `protocol_version`
* `snapshot_version`
* `selected_entry_ids`
* `covered_entry_ids`
* `continuation_source_kind`

---

## D. 권장 실행 순서

### Milestone 1 — 계약 고정

* OM-08
* OM-03

### Milestone 2 — 식별자/데이터 모델 고정

* OM-04
* OM-06

### Milestone 3 — 읽기/검색 경로 전환

* OM-01
* OM-07

### Milestone 4 — continuity 품질 보강

* OM-02
* OM-05

### Milestone 5 — 기본값 전환

* feature flag 제거
* old line-count reflection path deprecated
* old raw observation-string search hint path deprecated

---

## 태스크 목록 요약

### 바로 생성할 에픽

* `OM-08 Versioned protocol governance`
* `OM-03 Prompt contract v1`
* `OM-04 Canonical thread identity`
* `OM-06 Observation entry model v2`
* `OM-01 Search-visible snapshot`
* `OM-07 Hint compaction v2`
* `OM-02 Continuation state v2`
* `OM-05 Deterministic fallback engine v2`

### 각 에픽 공통 서브태스크 템플릿

* 설계 문서
* 타입/스키마 정의
* pure transform 구현
* AxiomMe adapter/store 통합
* feature flag
* fixture/golden 추가
* metrics/logging 추가
* migration/backfill
* shadow validation
* default rollout

---

## 최종 판단

이번 OM 개선은 “힌트를 몇 줄 더 붙이는 수준”이 아니라, **OM를 텍스트 문자열 묶음에서 versioned structured state로 승격하는 작업**으로 보는 것이 맞습니다. 현재 코드 구조도 그 방향을 허용합니다. `episodic`는 이미 pure contract/core 역할을 하고 있고, AxiomMe는 runtime boundary와 SQLite 상태 레이어를 가지고 있기 때문입니다. 따라서 장기안의 핵심은 다음 한 줄로 요약됩니다:

> **`episodic`를 OM 프로토콜 저장소로 고정하고, AxiomMe는 그 프로토콜을 materialize/search/render/apply 하는 실행 엔진으로 재편한다.** ([GitHub][2])

[1]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/Cargo.toml "raw.githubusercontent.com"
[2]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/om/mod.rs "raw.githubusercontent.com"
[3]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/client/search/mod.rs "raw.githubusercontent.com"
[4]: https://raw.githubusercontent.com/AxiomOrient/episodic/main/src/transform/observer/decision.rs "raw.githubusercontent.com"
[5]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/session/om/observer/llm.rs "raw.githubusercontent.com"
[6]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/session/om/observer/threading.rs "raw.githubusercontent.com"
[7]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/session/om/observer/response.rs "raw.githubusercontent.com"
[8]: https://raw.githubusercontent.com/AxiomOrient/episodic/main/src/transform/reflection/draft.rs "raw.githubusercontent.com"
[9]: https://raw.githubusercontent.com/AxiomOrient/episodic/main/src/prompt/system.rs "raw.githubusercontent.com"
