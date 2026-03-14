# AxiomSync 현재 상태 리뷰

## 1. 결론

- `f6986a8` 커밋은 저장소에 존재한다. 따라서 검토를 계속할 수 있다.
- 이 프로젝트의 현재 핵심 방향은 이미 비교적 선명하다.  
  **로컬-퍼스트 단일 런타임 + 단일 `context.db` + 메모리 복원형 검색 인덱스 + 운영자 CLI** 구조다.
- **지금 당장 SQLite를 교체하는 것은 권장하지 않는다.**  
  개선이 필요하다면 저장소/도메인 경계와 검색 계층의 설계를 더 단순하게 만드는 쪽이 우선이다.
- 따라서 이번 판단은 다음이다.  
  **“개선은 필요하지만, DB 교체가 아니라 코어 경계 정리와 SQLite 내부 강화(특히 FTS5/하이브리드 검색, 상태 경계, 성능 계측)가 우선”**

---

## 2. 이번 분석에서 실제로 확인한 것

### 2.1 저장소/문서 계약
직접 확인한 저장소 계약은 다음과 같다.

- 저장소는 **runtime library + operator CLI** 를 소유한다.
- canonical URI 는 `axiom://{scope}/{path}` 이다.
- canonical local store 는 `<root>/context.db` 이다.
- retrieval backend policy 는 `memory_only` 이다.
- persistence backend 는 SQLite 로 고정되어 있다.
- startup 시 persisted search state 로부터 **in-memory index 를 복원**한다.
- web/mobile companion 은 이 저장소 밖에 둔다.

즉, 이 저장소는 처음부터 **“작고 배포 쉬운 로컬 런타임”** 을 목표로 설계되었다.

### 2.2 핵심 코드 경로
직접 line-by-line 로 읽은 핵심 파일은 다음이다.

- `crates/axiomsync/src/client.rs`
- `crates/axiomsync/src/client/runtime.rs`
- `crates/axiomsync/src/fs.rs`
- `crates/axiomsync/src/uri.rs`
- `crates/axiomsync/src/index.rs`
- `crates/axiomsync/src/retrieval/engine.rs`
- `crates/axiomsync/src/retrieval/planner.rs`
- `crates/axiomsync/src/state/mod.rs`
- `crates/axiomsync/src/state/migration.rs`
- `crates/axiomsync/src/state/search.rs`
- `crates/axiomsync/src/state/queue.rs`
- `crates/axiomsync/src/session/mod.rs`
- `crates/axiomsync/src/session/lifecycle.rs`
- `crates/axiomsync/src/embedding.rs`
- `crates/axiomsync/src/main.rs`
- `crates/axiomsync/src/lib.rs`

핵심 확인 결과:

1. `AxiomSync` 가 단일 façade 로 동작한다.
   - rooted filesystem
   - `context.db`
   - `InMemoryIndex`
   - retrieval engine
   - ontology / parser / markdown edit gate
   를 함께 묶는다.

2. runtime 시작 시
   - filesystem bootstrap
   - scope tier 보장
   - persisted search docs + OM records 로부터 메모리 인덱스 복원
   - profile stamp drift 시 full reindex
   흐름을 가진다.

3. 검색은 이미 완전히 빈약한 수준이 아니다.
   - in-memory index 안에서
     - exact
     - dense
     - sparse
     - recency
     - path
     를 섞는 하이브리드 점수 구조가 존재한다.
   - retrieval planner 는 query intent / scope / session hint / OM hint 를 이용해 fanout query 를 만든다.

4. embedder 는 기본적으로 로컬 deterministic 계열(`semantic-lite`) 이고,  
   optional 한 `semantic-model-http` 경로가 있으며 실패 시 fallback 이 존재한다.  
   즉, 현재 구조는 “외부 벡터 DB 없이는 검색이 불가능”한 구조가 아니다.

5. SQLite 는 단순 key-value 저장용이 아니라 아래를 모두 맡고 있다.
   - system_kv
   - search_docs
   - search_doc_tags
   - outbox / queue checkpoint / reconcile_runs
   - trace index
   - OM 관련 상태/청크/스코프/스레드/reflection/checkpoint

6. migration SQL 안에서 이미 `PRAGMA journal_mode = WAL;` 이 적용된다.  
   즉, WAL 자체는 이미 설계에 들어가 있다.

---

## 3. 파일 단위 인벤토리

아래는 이번에 확인한 파일/디렉터리 인벤토리다.  
표기:
- **[직접확인]**: 내용을 직접 읽고 역할을 판단
- **[트리확인]**: GitHub tree 와 문서 계약으로 역할 분류

### 3.1 루트

- `Cargo.toml` — workspace manifest, 단일 member `crates/axiomsync` [직접확인]
- `Cargo.lock` — dependency lock [직접확인]
- `README.md` — 제품/런타임의 대외 설명 [문서확인]
- `docs/README.md` — 유지 문서 범위 정의 [문서확인]
- `docs/ARCHITECTURE.md` — 아키텍처 경계, runtime/search/state cutover [문서확인]
- `docs/API_CONTRACT.md` — 고정된 public/runtime contract [문서확인]
- `docs/BUILD_ARTIFACT_CONTROL.md` — build artifact 통제 [문서확인]

### 3.2 스크립트

- `scripts/check_prohibited_tokens.sh` — 금지 토큰 검사 [트리확인]
- `scripts/mirror_notice_gate.sh` — mirror notice gate [트리확인]
- `scripts/mirror_notice_router.sh` — mirror notice route [트리확인]
- `scripts/mirror_notice_router_smoke.sh` — router smoke [트리확인]
- `scripts/perf_regression_gate.sh` — 성능 회귀 gate [문서/스크립트확인]
- `scripts/quality_gates.sh` — fmt/clippy/test/audit 중심 품질 게이트 [문서/스크립트확인]
- `scripts/release_pack_strict_gate.sh` — strict release pack gate [문서/스크립트확인]

### 3.3 패키지 루트

- `crates/axiomsync/Cargo.toml` — crate feature/dependency 정의 [직접확인]
- `crates/axiomsync/README.md` — 단일 crate 의 책임 범위 설명 [문서확인]
- `crates/axiomsync/TEST_INTENT.md` — 테스트 의도 문서 [문서확인]

### 3.4 `src/` 최상위 파일

- `lib.rs` — crate export surface, public/private 모듈 경계 [직접확인]
- `main.rs` — CLI entrypoint [직접확인]
- `client.rs` — `AxiomSync` façade 와 bootstrap/runtime 진입점 [직접확인]
- `fs.rs` — rooted filesystem, symlink escape 방지, queue scope write 제한 [직접확인]
- `uri.rs` — `axiom://` URI 파싱/정규화/경계 [직접확인]
- `index.rs` — in-memory hybrid index [직접확인]
- `embedding.rs` — embedder runtime / fallback / profile stamp [직접확인]
- `context_ops.rs` — context operation 모음 [트리확인]
- `catalog.rs` — catalog 관련 [트리확인]
- `error.rs` — 공통 error contract [트리확인]
- `evidence.rs` — evidence 수집 관련 [트리확인]
- `host_tools.rs` — 외부 툴 인터페이스 [트리확인]
- `ingest.rs` — ingest 경로 [트리확인]
- `jsonl.rs` — JSONL 처리 [트리확인]
- `llm_io.rs` — LLM 입출력/endpoint 보조 [트리확인]
- `markdown_preview.rs` — markdown preview feature [트리확인]
- `mime.rs` — MIME 추론 [트리확인]
- `om_bridge.rs` — OM bridge [트리확인]
- `pack.rs` — pack/export 관련 [트리확인]
- `parse.rs` — parsing 지원 [트리확인]
- `quality.rs` — quality 보조 [트리확인]
- `queue_policy.rs` — queue 정책 [트리확인]
- `relation_documents.rs` — relation document 조립 [트리확인]
- `release_gate.rs` — release gate facade [트리확인]
- `security_audit.rs` — security audit [트리확인]
- `text.rs` — text utility [트리확인]
- `tier_documents.rs` — tier document 조립 [트리확인]

### 3.5 `src/` 주요 디렉터리

#### `cli/` [트리확인]
CLI argument/command parser 계층.
- `args.rs`
- `benchmark.rs`
- `document.rs`
- `eval.rs`
- `mod.rs`
- `ontology.rs`
- `parsers.rs`
- `queue.rs`
- `relation.rs`
- `release.rs`
- `security.rs`
- `session.rs`
- `tests.rs`
- `trace.rs`

#### `client/`
런타임 façade 의 실제 서비스 계층.

직접 확인:
- `runtime.rs` — runtime index restore, reindex, backend status, session/queue/OM bootstrap [직접확인]
- `search/backend.rs` — 현재 retrieval backend policy 가 `memory_only` 임을 enforcement [직접확인]

트리 확인:
- `indexing.rs`
- `markdown_editor.rs`
- `mirror_outbox.rs`
- `om_bridge.rs`
- `ontology.rs`
- `queue_reconcile.rs`
- `relation.rs`
- `request_log.rs`
- `resource.rs`
- `benchmark/*`
- `eval/*`
- `indexing/*`
- `mirror_outbox/*`
- `release/*`
- `search/*`
- `tests/*`
- `trace/*`

#### `commands/` [트리확인]
CLI 실행 dispatcher 계층.
- `handlers.rs`
- `mod.rs`
- `ontology.rs`
- `queue.rs`
- `support.rs`
- `tests.rs`
- `validation.rs`
- `web.rs`

#### `config/` [트리확인]
환경/검색/메모리/OM config.
- `env.rs`
- `indexing.rs`
- `memory.rs`
- `mod.rs`
- `om.rs`
- `search.rs`

#### `index/`
메모리 인덱스 구성요소.
- `exact.rs` — exact/fuzzy compact match 키 구성 [직접확인]
- `ancestry.rs` [트리확인]
- `filter.rs` [트리확인]
- `lifecycle.rs` [트리확인]
- `rank.rs` [트리확인]
- `search_flow.rs` [트리확인]
- `text_assembly.rs` [트리확인]

#### `models/` [트리확인]
런타임 DTO / request-response / benchmark/eval/search/session/release 모델.
- `benchmark.rs`
- `defaults.rs`
- `eval.rs`
- `filesystem.rs`
- `mod.rs`
- `queue.rs`
- `reconcile.rs`
- `release.rs`
- `search.rs`
- `session.rs`
- `trace.rs`

#### `om/`
OM runtime integration.
- `mod.rs` [트리확인]
- `failure.rs` [트리확인]
- `rollout.rs` [트리확인]
- `thread_identity.rs` [트리확인]
- `engine/` — vendored OM contract/transform/parse/prompt/context [문서/트리확인]
  - `addon.rs`
  - `context.rs`
  - `inference.rs`
  - `mod.rs`
  - `model.rs`
  - `pipeline.rs`
  - `xml.rs`
  - `addon/tests.rs`
  - `config/input.rs`
  - `config/mod.rs`
  - `config/resolve.rs`
  - `config/tests.rs`
  - `config/validate.rs`
  - `context/tests.rs`
  - `inference/tests.rs`
  - `model/tests.rs`
  - `parse/mod.rs`
  - `parse/sections.rs`
  - `parse/tests.rs`
  - `parse/thread.rs`
  - `parse/tokens.rs`
  - `pipeline/tests.rs`
  - `prompt/contract.rs`
  - `prompt/formatter.rs`
  - `prompt/mod.rs`
  - `prompt/parser.rs`
  - `prompt/system.rs`
  - `prompt/tests.rs`
  - `prompt/user.rs`
  - `transform/activation.rs`
  - `transform/helpers.rs`
  - `transform/mod.rs`
  - `transform/scope.rs`
  - `transform/snapshot.rs`
  - `transform/types.rs`
  - `transform/observer/*`
  - `transform/reflection/*`
  - `transform/tests/*`

#### `ontology/` [트리확인]
ontology schema/model/parse/validate/pressure.
- `mod.rs`
- `model.rs`
- `parse.rs`
- `pressure.rs`
- `validate.rs`

#### `release_gate/` [트리확인]
릴리즈/품질 계약 평가.
- `build_quality.rs`
- `contract_integrity.rs`
- `contract_probe.rs`
- `decision.rs`
- `episodic_semver.rs`
- `policy.rs`
- `test_support.rs`
- `tests.rs`
- `workspace.rs`
- `workspace_command.rs`

#### `retrieval/`
검색 fanout / budget / scoring.
- `engine.rs` — planner 결과 fanout 실행 및 hit bucket 분류 [직접확인]
- `planner.rs` — scope-aware query expansion planner [직접확인]
- `budget.rs` [트리확인]
- `config.rs` [트리확인]
- `expansion.rs` [트리확인]
- `mod.rs` [트리확인]
- `scoring.rs` [트리확인]
- `tests.rs` [트리확인]

#### `session/`
세션/메모리 lifecycle.
- `mod.rs` — Session aggregate [직접확인]
- `lifecycle.rs` — load/add_message/update_tool_part [직접확인]
- `archive.rs` [트리확인]
- `context.rs` [트리확인]
- `indexing.rs` [트리확인]
- `memory_extractor.rs` [트리확인]
- `meta.rs` [트리확인]
- `om.rs` [트리확인]
- `paths.rs` [트리확인]
- `tests.rs` [트리확인]
- `commit/*` [트리확인]
- `om/*` [트리확인]

#### `state/`
SQLite state store.
- `mod.rs` — connection/open/transaction façade [직접확인]
- `migration.rs` — schema/migration/integrity/OM migration [직접확인]
- `search.rs` — search_docs persist/list/remove [직접확인]
- `queue.rs` — outbox/checkpoint/requeue/statistics [직접확인]
- `om.rs` [트리확인]
- `promotion_checkpoint.rs` [트리확인]
- `tests.rs` [트리확인]
- `queue_lane.rs` [트리확인]
- `state/om/*` [트리확인]

### 3.6 테스트

루트 테스트:
- `tests/core_contract_fixture.rs`
- `tests/om_parity_fixtures.rs`
- `tests/om_runtime_behavior_validation.rs`
- `tests/process_contract.rs`
- `tests/release_contract_fixture.rs`

fixtures:
- `tests/fixtures/core_contract_fixture.json`
- `tests/fixtures/parity_cases.json`
- `tests/fixtures/release_contract_fixture.json`

client tests:
- `client/tests/benchmark_suite_tests.rs`
- `client/tests/core_editor_retrieval.rs`
- `client/tests/eval_suite_tests.rs`
- `client/tests/initialization_lifecycle.rs`
- `client/tests/mod.rs`
- `client/tests/om_bridge_contract.rs`
- `client/tests/ontology_enqueue.rs`
- `client/tests/queue_reconcile_lifecycle.rs`
- `client/tests/relation_trace_logs.rs`
- `client/tests/release_contract_pack_tracemetrics.rs`

---

## 4. 중립적 진단

### 4.1 현재 설계가 이미 좋은 점
1. **운영 단순성**
   - DB 서버가 없다.
   - 로컬 파일 + `context.db` 만 있으면 된다.
   - solo/small team 대상과 잘 맞는다.

2. **경계가 명시적**
   - rooted FS, `axiom://` URI, queue scope restriction, in-memory retrieval policy, no legacy discovery 가 모두 명시적이다.

3. **로컬 deterministic 검색 가능**
   - 기본 embedder 가 로컬 deterministic 이기 때문에 외부 SaaS 없이는 동작하지 않는 구조가 아니다.

4. **운영 품질 의식이 이미 존재**
   - quality gate / release pack strict gate / perf regression gate / TEST_INTENT 가 있다.

### 4.2 현재 설계의 핵심 위험
1. **한 패키지에 너무 많은 책임이 응집**
   - runtime
   - CLI
   - benchmark/eval
   - release gate
   - security audit
   - OM vendored engine
   - queue reconcile
   가 한 crate 안에 강하게 붙어 있다.

2. **SQLite 가 물리 저장소 하나로 너무 많은 bounded context 를 품고 있음**
   - 이 자체가 나쁜 것은 아니지만,
   - `document state / search projection / queue / OM / trace / release evidence`
     경계가 명확하지 않으면 이후 단순함이 깨진다.

3. **검색 품질을 높이고 싶을 때 외부 DB 로 급히 튈 유혹**
   - 하지만 지금은 그 단계가 아니다.
   - 먼저 SQLite 내부에서 할 수 있는 개선(FTS5/hybrid, projection 설계, 성능 계측)을 다 한 뒤 판단해야 한다.

---

## 5. 저장소 선택에 대한 판단

### 추천
- **기본 canonical store 는 계속 SQLite**
- **runtime hot path 는 계속 in-memory index**
- **다음 개선은 SQLite 교체가 아니라 SQLite 내부 강화**
  - FTS5 도입 가능성 검토
  - queue/search/restore 성능 측정
  - bounded context 명확화
  - 향후 sync 모드만 optional 로 분리

### 비추천
- 지금 당장 Postgres/pgvector 로 전환
- 지금 당장 Qdrant 를 canonical dependency 로 채택
- 지금 당장 sqlite-vec 를 핵심 필수 의존성으로 고정
- 검색 품질 문제를 구조 문제보다 먼저 DB 교체로 해결하려는 접근

---

## 6. 왜 “개선 문서”를 작성하는 쪽으로 판정했는가

이 프로젝트는 **충분히 가능성이 있지만**, 지금이 바로 아키텍처의 “방향을 못 박아야 하는 시점”이다.

지금 개선이 필요한 이유는 DB 자체보다 다음 때문이다.

- 코어 책임이 넓어지는 속도가 빠르다.
- state/migration/queue/OM 이 커지고 있다.
- small team 에게 진짜 중요한 것은 “더 강한 DB” 보다
  - 설치 즉시 실행
  - 장애 복구 단순성
  - 데이터 위치의 명료성
  - 검색 품질의 예측 가능성
  이다.

따라서 다음 문서들이 실제로 필요하다.

1. 청사진
2. 로드맵
3. 구현/개선 가이드
4. 작업 계획 및 태스크 백로그

---

## 7. 참고 소스

### 저장소
- https://github.com/AxiomOrient/AxiomSync
- https://github.com/AxiomOrient/AxiomSync/commit/f6986a8
- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/docs/ARCHITECTURE.md
- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/docs/API_CONTRACT.md
- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/crates/axiomsync/TEST_INTENT.md

### 기술 비교
- SQLite About: https://sqlite.org/about.html
- SQLite WAL: https://sqlite.org/wal.html
- SQLite FTS5: https://sqlite.org/fts5.html
- pgvector README: https://github.com/pgvector/pgvector
- Qdrant docs: https://qdrant.tech/documentation/quickstart/
- LanceDB quickstart: https://docs.lancedb.com/quickstart
- Turso embedded replicas: https://docs.turso.tech/features/embedded-replicas/introduction
- libSQL README: https://github.com/tursodatabase/libsql
- sqlite-vec README: https://github.com/asg017/sqlite-vec
