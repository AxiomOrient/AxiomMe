# AxiomSync 목표 청사진

## 1. 결론

AxiomSync 의 가장 좋은 미래 형태는 다음이다.

> **“SQLite 를 canonical state 로 유지하고, in-memory index 를 hot path 로 유지하며, 필요한 검색 품질 개선은 먼저 SQLite 내부(FTS5/hybrid)에서 해결한다. 외부 검색 엔진/원격 DB 는 선택 모드로만 추가한다.”**

즉, 제품의 본질은 **“로컬에서 바로 돌고, 망가져도 이해하기 쉽고, 데이터 위치가 명확한 agent context runtime”** 이다.

---

## 2. 제품 정체성

### 2.1 핵심 한 줄
AxiomSync 는 **AI agent 의 context 를 로컬에서 관리/확장/복원/검색하는 단일 런타임** 이다.

### 2.2 타깃 사용자
- 1인 기업
- 2~10인 규모의 소규모 팀
- 로컬 개발 환경 / 단일 VPS / 단일 워크스페이스 운영자
- “인프라를 운영하고 싶지 않은” AI product builder

### 2.3 해야 하는 일
1. 파일/문서/노트/세션 로그를 context 로 흡수
2. user / agent / session memory 를 유지
3. query 에 맞는 context 를 빠르게 확장
4. queue/outbox/reconcile 로 상태를 복원 가능하게 유지
5. export / release evidence / audit 를 제공
6. 나중에 필요하면 small-team sync 로 확장

### 2.4 하지 말아야 하는 일
- 처음부터 분산 검색 엔진이 되기
- 처음부터 다중 테넌트 SaaS 가 되기
- canonical state 를 외부 전용 vector DB 에 넘기기
- 문서 저장소와 검색 저장소의 소유권을 혼탁하게 만들기

---

## 3. 아키텍처 원칙

### 원칙 A — Source of Truth 를 3층으로 분리
1. **파일 시스템**
   - 사용자 문서/리소스의 원본
2. **SQLite (`context.db`)**
   - 런타임 상태, 검색 projection, queue, OM, checkpoint 의 canonical state
3. **In-memory index**
   - 완전히 disposable 한 hot projection

### 원칙 B — 외부 검색 엔진은 절대 canonical 이 아니다
- Qdrant/LanceDB/pgvector 등은 도입하더라도 **secondary index** 여야 한다.
- 시스템의 복원 가능성은 항상
  - FS
  - SQLite
  로만 보장되어야 한다.

### 원칙 C — no daemon by default
- 기본 배포는 별도 DB 서버/검색 서버/브로커/워크커가 없어야 한다.
- 단일 프로세스로 시작 가능해야 한다.

### 원칙 D — search 는 단계적으로 강화
1. 현재: in-memory hybrid retrieval
2. 다음: SQLite FTS5 기반 lexical/hybrid 강화
3. 이후: 필요 시 optional vector companion

### 원칙 E — team mode 는 local mode 를 깨지 않는다
- 팀 sync 가 필요해도 local mode 의 단순성을 해치면 안 된다.
- sync 모드는 add-on 이어야 한다.

---

## 4. 저장소 전략

## 4.1 권장안: SQLite + In-Memory + Optional FTS5

### 역할 분담
- `resources/`, `user/`, `agent/`, `session/` 파일/세션 소스 → FS
- search_docs / tags / queue / OM / checkpoints / traces → SQLite
- retrieval scoring / planner / hot projection → 메모리

### 왜 이 조합인가
- SQLite 는 self-contained / serverless / zero-configuration 이다.
- WAL 로 reader/writer concurrency 를 어느 정도 확보할 수 있다.
- FTS5/BM25 를 같은 파일 안에서 쓸 수 있다.
- 지금 저장소의 API contract 와 가장 잘 맞는다.

### 보강 포인트
- `search_docs` 를 FTS5 projection 과 연결
- lexical score + existing dense/sparse score 를 결합
- 검색 품질 향상은 DB 교체보다 먼저 같은 파일 안에서 해결

---

## 5. 대안 비교

| 대안 | 장점 | 치명적 비용 | 최종 판단 |
|---|---|---|---|
| **현행 SQLite + Memory** | 배포 단순, 데이터 위치 명확, 운영 난이도 낮음 | 검색 품질/부팅 성능은 직접 설계 필요 | **기본안 유지** |
| **SQLite + FTS5** | 동일 DB 안에서 BM25/lexical 강화 | projection 관리 필요 | **즉시 검토 가치 높음** |
| **sqlite-vec** | SQLite 안에서 vector 가능 | pre-v1, 핵심 의존성으로는 아직 이르다 | **실험용만** |
| **LanceDB** | embedded, local path, vector-native | 두 번째 로컬 DB 도입으로 복잡도 증가 | **후보지만 아직 이르다** |
| **Qdrant** | 벡터 검색 기능 강함 | Docker/서버/보안/운영 표면이 늘어남 | **현재 타깃에는 과함** |
| **Postgres + pgvector** | shared/team/server 환경 강함 | Postgres 운영/extension 관리 필요 | **현재 타깃과 불일치** |
| **libSQL/Turso** | local+remote sync story 좋음 | remote primary / sync model 설계 필요 | **team sync 단계에서 고려** |

---

## 6. 모드별 청사진

## 6.1 Mode A — Solo Local Runtime (기본)
- 단일 프로세스
- 단일 `context.db`
- 단일 rooted workspace
- 메모리 인덱스
- optional local model endpoint
- export/backup/pack 지원

이 모드가 제품의 정체성이다.

## 6.2 Mode B — Small Team Sync (후속)
- 각 사용자 로컬 runtime 유지
- canonical local state 유지
- sync 대상만 공유
- 후보 기술:
  - libSQL/Turso sync
  - 또는 명시적 outbox/pull sync

핵심은 **로컬 우선** 이다. 중앙집중형 SaaS 우선이 아니다.

## 6.3 Mode C — Search Companion (선택)
- corpus 가 커지거나 semantic retrieval 요구가 커질 때만
- external/vector engine 을 붙인다
- 단, source-of-truth 불변:
  - FS + SQLite 만 복원 기준

---

## 7. 내부 모듈 경계

## 7.1 Core Runtime
반드시 얇고 명확해야 하는 경계.

- URI / rooted FS
- context ingest
- search projection
- retrieval planner/engine
- session lifecycle
- SQLite state
- OM bridge 의 최소 runtime 부분

## 7.2 Operator / Lab
사용자 가치와 직접 연관되지만 코어에서 분리 가능한 영역.

- benchmark
- eval
- release evidence
- release gate
- security audit
- trace replay/report
- mirror/reconcile tooling

이 영역은 같은 저장소에 있어도 되지만, **core runtime 의 인지 부하를 올리지 않도록 경계가 분리되어야 한다.**

## 7.3 Vendored OM Engine
- 현재 구조상 필요한 도메인
- 그러나 코어 런타임과 결합도를 계속 측정해야 한다
- 공개 contract 와 변환기만 유지하고, 제품 핵심과 무관한 실험적 부분은 격리

---

## 8. 검색 아키텍처 목표

### 현재
- in-memory hybrid
- planner-based fanout
- scope-aware retrieval
- deterministic default embeddings

### 목표
**L0 / L1 / L2** 형태로 단순화한다.

- **L0**: exact path/name/header hit
- **L1**: SQLite FTS5 BM25 lexical hit
- **L2**: in-memory dense/sparse rerank
- **L3(optional)**: semantic/vector companion

이 순서가 중요한 이유:
- 작은 팀에게는 lexical quality + deterministic rerank 만으로도 상당수 케이스가 해결된다.
- 가장 비싼 external vector stack 을 마지막으로 미룰 수 있다.

---

## 9. 데이터 모델 원칙

### 문서
- FS 가 원본
- SQLite 는 projection 과 metadata
- projection 은 언제든 rebuild 가능해야 함

### 세션
- session scope 에 명시적 저장
- 메모리 승격/압축/checkpoint 정책을 테이블/문서로 명확히 함

### OM
- scope/session/thread 기준의 bounded context 분리
- reflection/continuation/event/applied checkpoint 를 명시적으로 유지
- 검색 projection 과 canonical OM state 를 혼동하지 않음

### Queue
- outbox 는 side-effect scheduler 이지 진실의 원천이 아님
- replay/reconcile 로 항상 회복 가능해야 함

---

## 10. 성공 지표

### 제품 지표
- 설치 후 첫 실행: 1분 이내
- 단일 workspace 백업/복구: 설명 가능해야 함
- 데이터 위치 설명: “문서는 여기, 상태는 여기, hot index 는 여기”
- 검색 결과 설명 가능성: 왜 선택됐는지 notes 로 남아야 함

### 기술 지표
- cold boot / warm boot / full reindex 시간 추적
- search p50/p95
- outbox backlog 처리 시간
- corruption 발생 시 recovery runbook 길이
- feature 하나 추가할 때 touched modules 수

---

## 11. 핵심 결정 요약

1. **SQLite 는 유지**
2. **외부 vector DB 전환은 보류**
3. **FTS5/hybrid 를 먼저 검토**
4. **core runtime 과 operator/lab 경계를 명확히 함**
5. **small-team sync 는 이후 선택 모드**
6. **FS + SQLite 외에는 source-of-truth 로 인정하지 않음**

---

## 12. 참고 소스

- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/docs/ARCHITECTURE.md
- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/docs/API_CONTRACT.md
- https://sqlite.org/about.html
- https://sqlite.org/wal.html
- https://sqlite.org/fts5.html
- https://github.com/pgvector/pgvector
- https://qdrant.tech/documentation/quickstart/
- https://docs.lancedb.com/quickstart
- https://docs.turso.tech/features/embedded-replicas/introduction
- https://github.com/tursodatabase/libsql
- https://github.com/asg017/sqlite-vec
