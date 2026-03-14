# AxiomSync 로드맵

## 1. 결론

로드맵은 **“DB 교체”가 아니라 “코어를 고정하고, SQLite 내부에서 검색/성능을 강화하고, 그 다음에만 sync/외부 검색을 선택적으로 추가”** 하는 순서가 맞다.

---

## 2. 우선순위 원칙

1. **제품 정체성 보존**
   - local-first
   - single-runtime
   - one-command deploy

2. **복잡도 후순위화**
   - team sync
   - external vector service
   - distributed search
   는 실제 필요가 확인되기 전까지 뒤로 미룬다.

3. **측정 후 변경**
   - boot time
   - reindex time
   - retrieval quality
   - queue throughput
   를 먼저 계측한다.

---

## 3. 단계별 계획

## Phase 0 — 방향 고정 (즉시)

### 목표
- “무엇이 코어인지”를 문서와 테스트로 고정
- SQLite 교체 논의를 일단 종료
- 측정 기준선 확보

### 할 일
- 아키텍처 청사진 승인
- source-of-truth 규칙 명문화
- boot/reindex/search 기준선 벤치 정의
- queue/search/OM bounded context 표 작성
- public API/CLI surface 를 “core” 와 “ops” 로 분류

### 종료 조건
- 팀 내에서 “AxiomSync 는 무엇인가?”에 대한 설명이 한 문장으로 합의됨
- DB 교체 대신 유지/확장 전략이 합의됨

---

## Phase 1 — 코어 경계 정리

### 목표
- runtime core 와 operator/lab 기능의 인지 부하 분리

### 할 일
- 모듈 ownership map 작성
- `core runtime`, `ops/lab`, `vendored OM` 경계 문서화
- 가능하면 다음 중 하나 선택
  1. 같은 crate 안에서 모듈/feature 경계 강화
  2. 안정화 후 `core` 와 `cli/ops` 를 2-crate 로 분리
- CLI 서브커맨드를 user-facing / operator-facing 로 재구성
- release/eval/security/benchmark 를 runtime hot path 에서 분리

### 종료 조건
- feature 추가 시 touched module 수 감소
- runtime 관련 테스트와 release/lab 테스트가 논리적으로 분리

---

## Phase 2 — 검색 품질/성능 강화 (SQLite 내부)

### 목표
- 외부 검색 엔진 없이도 small-team 용도로 충분한 검색 품질 확보

### 할 일
- `search_docs` 기반 FTS5 prototype
- BM25 + existing hybrid score 조합 실험
- exact/path/header boost 규칙 정리
- query plan notes 에 lexical/dense/sparse contribution 기록
- cold restore / warm search / full reindex 성능 계측
- queue fetch 패턴과 search restore 패턴의 SQL index 점검

### 종료 조건
- corpus 규모가 늘어도 p95 search 와 cold boot 예산이 관리됨
- “이 정도면 외부 검색 엔진이 없어도 된다”는 기준이 수치로 설명됨

---

## Phase 3 — 팀 동기화 옵션

### 목표
- local-first 본질을 깨지 않고 small-team sync 를 추가

### 할 일
- sync 요구사항 정의
  - 누가 쓰는가
  - 동시성 모델
  - conflict 모델
  - offline 우선 여부
- 후보 검토
  - outbox 기반 pull/push
  - libSQL/Turso
- 공유 가능한 scope 와 로컬 전용 scope 분리
- backup/restore/sync failure runbook 작성

### 종료 조건
- sync 기능이 local mode 를 복잡하게 만들지 않음
- 운영자가 “네트워크가 죽어도 로컬은 계속 쓸 수 있다”고 확신할 수 있음

---

## Phase 4 — 선택적 외부 검색 Companion

### 목표
- 정말 필요할 때만 vector/search companion 도입

### 도입 조건
아래 중 2개 이상 충족될 때만 고려:
- corpus 가 현 구조의 cold boot 예산을 지속적으로 초과
- semantic retrieval 품질이 FTS5 + hybrid 로 충분히 안 나옴
- team sync 이후 shared search index 요구가 커짐
- 운영자가 별도 서비스를 감당할 수 있음

### 후보
- LanceDB: embedded/local second store
- Qdrant: service/vector engine
- pgvector: shared Postgres/team mode

### 종료 조건
- external engine 을 빼도 FS + SQLite 만으로 완전 복구 가능

---

## 4. 하지 말아야 할 순서

다음 순서는 피해야 한다.

1. 검색 품질 이슈 발생
2. 바로 vector DB 도입
3. canonical state 도 함께 외부화
4. sync / queue / restore / backup 설계가 꼬임

이 순서는 small-team 제품에서 거의 항상 복잡도를 폭발시킨다.

---

## 5. 마일스톤

## M1 — Core Contract Freeze
- 청사진 문서 반영
- source-of-truth 규칙 확정
- ownership map 확정

## M2 — Search Quality v1
- FTS5 prototype
- hybrid scoring 실험
- query note 가시화

## M3 — Performance Baseline v1
- boot/reindex/search benchmark
- queue throughput measurement
- restore hot path profiling

## M4 — Sync Feasibility
- local-only vs sync mode contract
- shared scope model
- failure semantics

## M5 — Optional Companion Decision
- external search/storage companion 필요 여부 결정

---

## 6. 리스크와 대응

| 리스크 | 설명 | 대응 |
|---|---|---|
| 코어가 계속 넓어짐 | runtime 와 lab 기능이 섞임 | ownership map, module boundary, test 분리 |
| SQLite 가 병목으로 오해됨 | 실제 병목은 검색/복원/경계일 수 있음 | 계측 먼저, DB 교체 나중 |
| 검색 품질 미달 | semantic 요구가 늘어날 수 있음 | FTS5 + hybrid + rerank 먼저 |
| sync 가 본질을 망침 | small-team 요구를 과장하면 SaaS 구조가 됨 | local-first 불변 원칙 유지 |
| OM 복잡도 증가 | vendored engine 이 코어를 잠식 | contract/transform boundary 유지 |

---

## 7. 의사결정 게이트

### Gate A — SQLite 유지 여부
다음이 모두 참이면 유지:
- local-first 가 핵심 가치
- 단일 호스트 운영이 주력
- boot/search 성능이 계측상 감당 가능
- 복구 단순성이 중요

### Gate B — FTS5 도입 여부
다음이 참이면 도입:
- lexical relevance 강화 필요
- 같은 DB 안에서 품질 개선 가능
- 두 번째 DB 를 피하고 싶음

### Gate C — external companion 여부
다음이 참이면 검토:
- FTS5 + current hybrid 로도 부족
- 운영 복잡도 증가를 감당 가능
- 공유/원격/대규모 요구가 실제로 존재

---

## 8. 최종 방향

로드맵의 요지는 단순하다.

> **지금은 SQLite 를 버릴 시점이 아니라, SQLite 를 중심으로 코어를 더 선명하게 만들 시점이다.**

---

## 9. 참고 소스

- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/docs/ARCHITECTURE.md
- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/docs/API_CONTRACT.md
- https://sqlite.org/about.html
- https://sqlite.org/wal.html
- https://sqlite.org/fts5.html
- https://docs.turso.tech/features/embedded-replicas/introduction
- https://qdrant.tech/documentation/quickstart/
- https://docs.lancedb.com/quickstart
- https://github.com/pgvector/pgvector
