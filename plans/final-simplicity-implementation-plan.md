# AxiomSync Final Simplicity Implementation Plan

> Archived note (2026-03-12): this document predates the current repository direction.
> Current execution truth for this repo is SQLite-only persistence, canonical store path `context.db`,
> preserved `axiom://` URIs, and external mobile FFI companions. Do not treat the backend-abstraction
> items below as the active delivery contract for this repository without an explicit replanning pass.

## 1. Goal

AxiomSync의 현재 강점인 `local-first + deterministic + explicit state transition`을 유지한 채, 아래 네 가지를 최종 형태로 완성한다.

1. 현재 기본 DB는 계속 SQLite로 유지한다.
2. 그러나 런타임 오케스트레이션은 concrete `SqliteStateStore`가 아니라 `StateStore` 계약에 의존하도록 바꾼다.
3. 모든 핵심 변경 경로에 `actor/work/approval` 계약을 도입하고 `agent -> user memory` 경계를 명시적 정책으로 올린다.
4. 인덱스 drift 판단을 `mtime + size`로 보강하고, 문서·테스트·품질 게이트를 새 구조와 동기화한다.

이 계획은 “더 많은 기능”이 아니라 “경계를 제품의 1급 계약으로 올리는 것”에 집중한다.

## 2. Validation Verdict

## 2.1 채택

- 현재 구조를 `fs + sqlite + in-memory index`로 보는 판단은 타당하다.
- `queue`가 내부 스코프이며 비시스템 쓰기 금지라는 전제는 타당하다.
- request log에 actor/work/approval 문맥이 없다는 지적은 타당하다.
- index drift가 현재 `mtime` 단일 비교라는 지적은 타당하다.
- `AxiomSync`와 `Session`이 concrete `SqliteStateStore`를 직접 들고 있으므로, DB 교체 준비가 되어 있지 않다는 지적도 타당하다.

## 2.2 조정 후 채택

- DB는 지금 당장 바꾸지 않는다.
- 대신 `StateStore` seam을 도입해 “기본은 SQLite, 그러나 오케스트레이션 수정 없이 대체 backend PoC를 꽂아볼 수 있는 상태”를 최종 목표로 둔다.
- 이 seam은 `StateStore trait + SQLite concrete factory + state injection constructor + conformance harness` 조합으로 만든다.

## 2.3 제외

- SurrealDB production backend 구현은 이번 범위에서 제외한다.
- runtime backend selector를 먼저 노출하는 작업은 제외한다.
  - 두 번째 backend가 없는 상태에서 selector를 넣으면 dead config만 생긴다.
- SQL/graph/vector를 아우르는 범용 추상 쿼리 계층은 제외한다.
- approval을 전역 정책 엔진으로 확장하는 작업은 제외한다.
  - approval은 이번 범위에서 “감사 가능한 계약 필드”다.

## 3. Repo-Grounded Evidence

- 아키텍처와 저장 책임 분리는 [README.md](../README.md), [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md), [crates/axiomsync/src/client.rs](../crates/axiomsync/src/client.rs)에 이미 명시되어 있다.
- `queue`는 내부 스코프이며 비시스템 쓰기 금지다: [docs/API_CONTRACT.md](../docs/API_CONTRACT.md), [crates/axiomsync/src/fs.rs](../crates/axiomsync/src/fs.rs).
- request log는 현재 actor/work/approval를 저장하지 않는다: [crates/axiomsync/src/models/trace.rs](../crates/axiomsync/src/models/trace.rs), [crates/axiomsync/src/client/request_log.rs](../crates/axiomsync/src/client/request_log.rs).
- memory category는 이미 `user`와 `agent`를 데이터 모델 차원에서 분리하고 있다: [crates/axiomsync/src/session/commit/promotion.rs](../crates/axiomsync/src/session/commit/promotion.rs).
- drift 판단은 현재 `mtime`만 비교한다: [crates/axiomsync/src/client/runtime.rs](../crates/axiomsync/src/client/runtime.rs), [crates/axiomsync/src/state/mod.rs](../crates/axiomsync/src/state/mod.rs).
- queue 재시도/백오프/processing timeout recovery는 이미 존재한다: [crates/axiomsync/src/queue_policy.rs](../crates/axiomsync/src/queue_policy.rs), [crates/axiomsync/src/state/queue.rs](../crates/axiomsync/src/state/queue.rs), [crates/axiomsync/src/client/queue_reconcile.rs](../crates/axiomsync/src/client/queue_reconcile.rs).
- 문서 저장은 ETag 검증, atomic write, reindex 실패 시 rollback을 이미 갖고 있다: [crates/axiomsync/src/client/markdown_editor.rs](../crates/axiomsync/src/client/markdown_editor.rs).
- 세션 append 이후 OM 반영은 best-effort이고 dead-letter diagnostics를 남긴다: [crates/axiomsync/src/session/lifecycle.rs](../crates/axiomsync/src/session/lifecycle.rs).
- `AxiomSync`와 `Session`은 현재 concrete `SqliteStateStore`를 직접 소유한다: [crates/axiomsync/src/client.rs](../crates/axiomsync/src/client.rs), [crates/axiomsync/src/session/mod.rs](../crates/axiomsync/src/session/mod.rs).

## 4. Final Delivery Contract

최종 완료 기준은 아래와 같다.

1. 런타임 오케스트레이션은 `SqliteStateStore`가 아니라 `StateStoreHandle`에 의존한다.
2. SQLite는 기본 backend로 유지되고, open/migrate/permission hardening은 concrete SQLite 경로에 남는다.
3. `StateStore` 계약은 실제 오케스트레이션이 쓰는 persistence 기능만 포함하며 backend-agnostic 타입만 노출한다.
4. one-off SQLite maintenance/admin 경로는 core `StateStore` trait 밖에 남는다.
5. 기본 SQLite constructor는 normal runtime용 `StateStoreHandle`과 SQLite 전용 admin companion을 함께 구성한다.
6. `AxiomSync::new_with_state(...)` 또는 동등한 injection 경로가 존재해 backend PoC를 꽂을 수 있다.
7. injected backend에는 SQLite admin companion이 없으며 SQLite 전용 maintenance/admin 메서드는 명시적 unsupported/validation 에러로 실패한다.
8. `StateStore` conformance harness와 state injection smoke test가 존재해 SQLite와 교체 준비 상태를 검증한다.
9. 핵심 public mutation 경로는 `OperationContext`를 받는 정식 surface를 갖는다.
10. request log에는 actor/work/approval 정보가 남는다.
11. queue와 session promotion 경로에서도 work correlation이 끊기지 않는다.
12. `agent` actor에 의한 `axiom://user/memories/...` 직접 쓰기는 설정으로 차단할 수 있다.
13. 인덱스 상태는 `mtime + size`를 저장하고 런타임 drift 판단도 이 둘을 기준으로 수행한다.
14. README, API contract, architecture, usage playbook, tests, quality gates가 새 계약과 일치한다.

## 5. Final Target Shape

## 5.1 State Store Boundary Contract

런타임 관점의 핵심 변경은 “DB를 바꾸는 것”이 아니라 “DB 경계를 concrete type에서 contract로 바꾸는 것”이다.

최종 구조:

- `state/api.rs`
  - concern별 subtrait 정의
  - composite `StateStore` 정의
  - `StateStoreHandle = Arc<dyn StateStore>` alias 정의
- `state/mod.rs`
  - trait API re-export
  - concrete `SqliteStateStore` re-export
- `client.rs`
  - `AxiomSync`는 `StateStoreHandle` 보유
- `session/mod.rs`
  - `Session`도 `StateStoreHandle` 보유

원칙:

- trait에는 backend-agnostic 타입만 둔다.
- `open()`, `migrate()`, SQLite permission hardening 같은 lifecycle은 concrete SQLite에 남긴다.
- trait object로 지우기 전에 concrete backend가 자기 migration을 끝내야 한다.

## 5.2 State Store Method Clusters

`StateStore`는 한 번에 무작정 거대한 trait로 만들지 않는다. concern별 subtrait를 두고 composite로 묶는다.

필수 cluster:

1. `SystemAndTraceStore`
   - system values
   - trace index
2. `IndexCatalogStore`
   - index state
   - search document catalog
3. `QueueStateStore`
   - outbox
   - retry/requeue
   - checkpoints
   - queue diagnostics
4. `OmStateStore`
   - OM record
   - observation chunks
   - reflection/apply CAS
   - OM scope/metrics
5. `PromotionCheckpointStore`
   - memory promotion checkpoint lifecycle

핵심 규칙:

- `AxiomSync`와 `Session`이 실제로 호출하는 메서드만 trait에 올린다.
- 테스트 전용 helper, SQLite 내부 보조 함수, rusqlite-specific details는 trait에 올리지 않는다.
- `om_v2_migration_*`, reconcile run bookkeeping, raw schema migration, SQLite hardening 같은 one-off maintenance/admin API는 core trait 밖에 둔다.

## 5.3 SQLite Default Backend Contract

SQLite는 계속 기본값이다.

최종 상태:

- `SqliteStateStore::open(path)`는 계속 concrete constructor로 남는다.
- SQLite migration, schema validation, permission hardening은 concrete layer에서 수행한다.
- `open_default_state_store(root)` 또는 동등한 helper가 SQLite를 열고 migration을 끝낸 뒤 `StateStoreHandle`로 넘긴다.
- 기본 constructor는 runtime orchestration용 `StateStoreHandle`과 SQLite 전용 maintenance/admin 호출용 optional admin companion을 함께 묶는다.
- `new_with_state` 같은 injection 경로는 admin companion 없이 runtime seam만 받는다.

즉, 런타임은 trait에 의존하지만 제품 기본값은 여전히 SQLite다. SQLite 전용 admin 메서드는 backend-agnostic trait에 올리지 않고 companion으로만 접근한다.

## 5.4 Backend PoC Readiness Contract

이번 delivery의 목적은 “SurrealDB backend shipping”이 아니라 “교체 PoC가 가능해지는 상태”다.

PoC readiness 기준:

- `AxiomSync`/`Session`이 concrete SQLite type에 묶여 있지 않다.
- backend candidate는 `StateStore` 구현과 injection constructor만으로 런타임에 꽂을 수 있다.
- SQLite는 conformance harness의 기준 구현(reference implementation) 역할을 한다.
- SQLite-only admin surface는 injected backend에서 비활성화되며 명시적 unsupported/validation 에러를 반환한다.
- test-only state injection smoke가 최소 1개 존재해 교체 경로가 문서가 아니라 실행 가능한 코드로 검증된다.

이 범위에서는 runtime backend selector를 추가하지 않는다.

## 5.5 Operation Context Contract

새 canonical mutation contract는 다음 네 필드를 갖는다.

- `actor_kind`
  - `user | agent | system | legacy`
- `actor_id: Option<String>`
- `work_id: Option<String>`
- `approval: Option<ApprovalRef>`

`ApprovalRef` 최소 구조:

- `approved_by: String`
- `reason: String`

원칙:

- `OperationContext`는 low-level FS/state가 아니라 `AxiomSync` public mutation boundary와 `Session` mutation boundary에서 강제한다.
- 기존 무문맥 API는 호환용 thin wrapper로 남기고 내부에서 `actor_kind=legacy` 컨텍스트를 만들어 새 canonical surface로 위임한다.

## 5.6 Policy Contract

이번 범위의 강제 정책은 하나만 둔다.

- `agent` actor가 `axiom://user/memories/...`에 직접 쓰는 경로를 설정으로 차단할 수 있어야 한다.

정책 형태:

- `AppConfig.authz.enforce_agent_user_memory_boundary: bool`
- 기본값은 `false`
- `true`일 때는 `actor_kind=agent`인 promotion/apply 경로에서 `PermissionDenied`를 반환한다.

Approval은 이번 범위에서 기록 필드다.

## 5.7 Drift Contract

`index_state`는 최종적으로 아래를 저장한다.

- `content_hash`
- `mtime_nanos`
- `size_bytes`
- `indexed_at`
- `status`

런타임 정책:

- startup drift check는 `mtime_nanos != stored_mtime || size_bytes != stored_size`일 때 drift로 판단한다.
- legacy row에서 `size_bytes`가 비어 있거나 sentinel이면 drift로 간주해 1회 재색인한다.
- content hash는 기존처럼 실제 색인 변경 여부 판단과 state update에 계속 사용한다.

## 5.8 Queue / Work Correlation Contract

- queue enqueue 시점에 `OperationContext` 전체를 `context_json`으로 함께 저장한다.
- replay 시 생성되는 request log와 dead-letter 경로는 원본 `work_id`를 복원해 남긴다.
- session promotion/commit도 동일 `work_id`를 받으면 request log 상에서 연결 가능해야 한다.

## 6. Scope

## 6.1 포함

- `StateStore` seam 도입
- SQLite concrete factory 유지
- state injection constructor
- state conformance harness
- request log contract
- session promotion/commit mutation path
- queue enqueue/replay context propagation
- SQLite migration for `index_state`
- config/env surface for boundary enforcement
- docs/tests/quality gate sync

## 6.2 제외

- SurrealDB production backend
- runtime backend selector
- 범용 DB abstraction framework
- 범용 RBAC/ACL 시스템
- queue 전용 새 서브커맨드
- 원격 멀티유저 동기화

## 7. File-Level Change Map

## 7.1 State Boundary

- `crates/axiomsync/src/state/api.rs`
  - subtrait와 composite `StateStore` 정의
  - `StateStoreHandle` alias 정의
- `crates/axiomsync/src/state/mod.rs`
  - trait API re-export
  - concrete SQLite 구현 re-export
- `crates/axiomsync/src/client.rs`
  - `AxiomSync.state`를 `StateStoreHandle`로 전환
  - state injection constructor 추가
  - optional SQLite admin companion 보유
- `crates/axiomsync/src/session/mod.rs`
  - `Session.state`를 `StateStoreHandle`로 전환

## 7.2 SQLite Concrete Path

- `crates/axiomsync/src/state/mod.rs`
  - `SqliteStateStore::open` 유지
- `crates/axiomsync/src/state/migration.rs`
  - concrete SQLite migration 유지
- `crates/axiomsync/src/client/runtime.rs`
  - SQLite-specific maintenance/admin surface는 admin companion으로 유지
  - injected backend에서 unavailable 시 명시적 unsupported/validation 에러 반환
- `crates/axiomsync/src/state/tests.rs`
  - SQLite concrete open/migration 계약 유지 검증

## 7.3 Logging / Queue / Session

- `crates/axiomsync/src/models/`
  - `OperationContext`, `ActorKind`, `ApprovalRef` 추가
  - `RequestLogEntry` 확장
- `crates/axiomsync/src/client/request_log.rs`
  - logging helper가 `OperationContext`를 받도록 변경
- `crates/axiomsync/src/state/queue.rs`
  - outbox row에 `context_json` 추가
- `crates/axiomsync/src/client/queue_reconcile.rs`
  - replay log에 context 복원
- `crates/axiomsync/src/session/*`
  - promote/commit/apply 경로에 context thread-through

## 7.4 Mutation Surfaces

- `crates/axiomsync/src/client/resource.rs`
  - add/import/write 계열 mutation entrypoints에 canonical `_with_context` 추가
- `crates/axiomsync/src/client/markdown_editor.rs`
  - save 계열에 canonical `_with_context` 추가
- `crates/axiomsync/src/client/relation.rs`
  - mutation 경로에 context 추가
- `crates/axiomsync/src/client/ontology.rs`
  - enqueue action 경로에 context 추가
- `crates/external mobile FFI project/src/lib.rs`
  - mutation surface 노출 시 context/state injection 계약 반영

## 7.5 Drift / Docs / Quality

- `crates/axiomsync/src/state/mod.rs`
  - `upsert/get/list` accessor 확장
- `crates/axiomsync/src/client/indexing.rs`
  - file metadata size 수집 및 state update 반영
- `crates/axiomsync/src/client/indexing/helpers.rs`
  - `index_state_changed` 확장
- `crates/axiomsync/src/client/runtime.rs`
  - startup drift 판단 확장
- `README.md`, `docs/API_CONTRACT.md`, `docs/ARCHITECTURE.md`, `docs/USAGE_PLAYBOOK.md`
  - state boundary, PoC readiness, mutation contract, drift contract 반영

## 8. Critical Path

1. `StateStore` boundary와 concrete SQLite 책임 분리를 먼저 고정한다.
2. `AxiomSync`와 `Session`을 `StateStoreHandle` 기반으로 전환한다.
3. SQLite 구현을 trait 뒤에 다시 연결하고 conformance harness를 만든다.
4. 그 위에서 `OperationContext`와 authz 경계를 올린다.
5. queue context propagation을 넣는다.
6. `index_state.size_bytes` migration과 drift check 변경을 넣는다.
7. docs/tests/gates를 동기화한다.

이 순서를 바꾸면 문제가 생긴다.

- trait seam 없이 mutation contract를 먼저 올리면 backend 준비 작업이 다시 대규모 리팩터링이 된다.
- injection constructor 없이 PoC readiness를 주장할 수 없다.
- migration 없이 drift check를 먼저 바꾸면 기존 DB가 깨진다.

## 9. Decision Gates

### Gate A. State Store Boundary Freeze

확인 항목:

- trait가 orchestration seam까지만 캡슐화하는가
- concrete SQLite lifecycle이 trait 밖에 남는가

Pass:

- `AxiomSync`와 `Session`은 normal runtime path에서 `StateStoreHandle`만 본다
- `open/migrate/harden`는 SQLite concrete path에 남는다
- SQLite-only maintenance/admin surface는 optional admin companion으로만 접근된다
- trait에 rusqlite-specific 타입이 없다

Fail 시 조치:

- trait 범위를 줄인다
- runtime selector와 backend config 추가를 금지한다

### Gate B. Context Contract Freeze

확인 항목:

- `OperationContext` 필드 집합이 최소이면서 audit 목적을 충족하는가

Pass:

- `actor_kind`, `actor_id`, `work_id`, `approval` 외 필드가 추가되지 않는다
- approval은 기록 전용이다

Fail 시 조치:

- 설계를 줄이고 정책 범위를 더 넓히지 않는다

### Gate C. Queue Context Persistence

확인 항목:

- enqueue -> replay -> request log -> dead-letter까지 `work_id`가 유지되는가

Pass:

- 원본 enqueue 문맥이 replay log에 복원된다
- queue migration이 기존 outbox row를 안전하게 읽는다

Fail 시 조치:

- `context_json` 구조를 더 줄이고 outbox schema 필드는 늘리지 않는다

### Gate D. Drift Migration Safety

확인 항목:

- 기존 DB에서 `size_bytes` migration 후 startup이 안전한가

Pass:

- 기존 행에서 panic/deserialize failure가 없다
- 1회 재색인 후 steady state가 된다

Fail 시 조치:

- startup full reindex fallback을 명시적으로 강제한다

### Gate E. Contract Synchronization

확인 항목:

- API 문서, README, usage playbook, dependency contract가 서로 일치하는가

Pass:

- `docs/API_CONTRACT.md`의 dependency revision이 실제 `Cargo.toml`과 일치한다
- state boundary와 mutation contract가 모든 문서에서 동일하게 설명된다

Fail 시 조치:

- 문서 정합성 수정이 코드 반영보다 우선한다

## 10. Verification Plan

## 10.1 Automated Tests

- state boundary
  - `AxiomSync`와 `Session`이 concrete type 없이 compile되고 동작해야 한다
  - SQLite backend가 conformance harness를 통과해야 한다
- request log backward compatibility
  - old JSONL row를 새 `RequestLogEntry`로 읽을 수 있어야 한다
- context propagation
  - document save / ontology enqueue / replay / promotion에서 동일 `work_id`가 유지돼야 한다
- authz
  - `enforce_agent_user_memory_boundary=false`면 기존 동작 유지
  - `true`면 `agent`가 user memory write 시 `PermissionDenied`
- drift
  - same mtime + different size 상황에서 drift가 감지돼야 한다
  - migration 직후 legacy row가 안전하게 처리돼야 한다
- queue
  - retry / dead-letter / recovered processing 경로에서 context가 유지돼야 한다

## 10.2 Gate Commands

- `cargo test -p axiomsync`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `bash scripts/quality_gates.sh`

## 11. Risks and Controls

- 리스크: trait가 `SqliteStateStore`의 거대한 거울이 될 수 있다.
  - 통제: concern별 subtrait로 쪼개고 orchestration에서 실제 쓰는 메서드만 올린다.
- 리스크: concrete migration/open까지 trait 안으로 밀어 넣으면 abstraction leakage가 생긴다.
  - 통제: backend lifecycle은 concrete path에 남긴다.
- 리스크: second backend가 없는데 selector를 추가하면 dead config만 생긴다.
  - 통제: injection constructor만 만들고 selector는 만들지 않는다.
- 리스크: queue schema 변경과 state seam 변경이 동시에 들어와 테스트 churn이 커질 수 있다.
  - 통제: state boundary conformance를 먼저 고정하고 나머지 변경을 그 위에 올린다.
- 리스크: approval이 정책 엔진으로 확대될 수 있다.
  - 통제: 이번 범위에서는 기록 필드로만 유지한다.

## 12. Explicit Non-Goals

- 새 production DB backend 도입
- runtime backend selector
- async runtime 전환
- 범용 권한 프레임워크
- queue 대시보드
- 벡터/그래프 검색 재설계

## 13. Completion Standard

이 계획의 최종 완료는 아래 상태다.

- 현재 제품은 여전히 SQLite로 동작한다.
- 그러나 런타임 오케스트레이션은 concrete SQLite에 묶여 있지 않다.
- backend PoC는 `StateStore` 구현과 injection constructor만으로 시도할 수 있다.
- 변경 원인과 주체를 로그에서 추론 없이 확인할 수 있다.
- `agent`와 `user` 경계가 데이터 모델뿐 아니라 mutation contract에도 존재한다.
- drift 탐지가 더 안전해졌지만 여전히 저비용이다.
- 문서와 테스트가 실제 코드와 어긋나지 않는다.
