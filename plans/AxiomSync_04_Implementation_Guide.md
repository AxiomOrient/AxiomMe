# AxiomSync 구현 가이드 및 태스크 계획

## 1. 결론

구현 우선순위는 아래 순서가 맞다.

1. **코어 경계 명확화**
2. **성능 기준선 측정**
3. **SQLite 내부 개선**
4. **FTS5/hybrid prototype**
5. **그 다음에만 sync / external companion 검토**

---

## 2. 즉시 실행 태스크 (P0)

## T01 — Ownership Map 작성
### 목적
모듈/파일이 어느 bounded context 에 속하는지 고정.

### 산출물
- `docs/OWNERSHIP_MAP.md`

### 최소 내용
- core runtime
- ops/lab
- vendored OM
- shared model/config

### 완료 기준
- 새 기능 추가 시 소속 영역을 즉시 결정 가능

---

## T02 — Runtime 성능 기준선 추가
### 목적
추측 대신 수치로 판단.

### 측정 항목
- cold boot time
- warm boot time
- full reindex time
- first search latency
- steady-state p50/p95 search
- queue fetch/replay throughput

### 구현 제안
- 기존 benchmark/perf gate 에 아래 시나리오 추가
  - small corpus
  - medium corpus
  - stress corpus
- 출력은 JSON + markdown summary

### 완료 기준
- 변경 전/후 비교가 자동화됨

---

## T03 — SQLite access pattern 리뷰
### 목적
현재 SQL hot path 의 구조적 병목 제거.

### 직접 관찰된 후보
- `outbox` 조회는 `status`, `next_attempt_at`, `id` 순 패턴이 보인다.
- `search_docs` restore 는 전체 문서를 읽어 메모리 인덱스를 재구성한다.
- WAL 은 이미 켜져 있다.

### 작업
- `EXPLAIN QUERY PLAN` 기반으로 hot query 검증
- `busy_timeout` 실험 추가
- write batch 를 transaction 으로 더 묶을 수 있는지 확인
- outbox 관련 composite index 후보 검토

### index 후보 예시
```sql
CREATE INDEX IF NOT EXISTS idx_outbox_status_next_attempt_id
ON outbox(status, next_attempt_at, id);
```

### 완료 기준
- queue backlog 상황에서 fetch latency 가 안정적

---

## T04 — Query Plan Telemetry 정리
### 목적
검색 결과를 설명 가능하게 만든다.

### 작업
- query plan note 에 아래를 남김
  - backend
  - lexical contribution
  - dense contribution
  - sparse contribution
  - reranker mode
  - threshold/min_match_tokens
- 실패/드리프트 시 reindex reason note 기록

### 완료 기준
- 왜 특정 결과가 상위로 왔는지 사람이 설명 가능

---

## 3. 단기 구현 태스크 (P1)

## T10 — Core / Ops 경계 문서화
### 목적
인지 부하 감소.

### 방식
우선 문서 경계부터 만든다.
- Core:
  - fs
  - uri
  - ingest
  - index
  - retrieval
  - session
  - state
  - minimal OM runtime bridge
- Ops:
  - benchmark
  - eval
  - release gate
  - security audit
  - trace/report

### 완료 기준
- PR 단위 변경이 core 와 ops 중 어디에 속하는지 즉시 판단 가능

---

## T11 — CLI surface 재구성
### 목적
사용자용 명령과 운영자용 명령을 구분.

### 제안
- `axiomsync <user-facing>`
- `axiomsync ops <...>`

또는
- help 출력에서 core / ops 섹션 분리

### 완료 기준
- 처음 쓰는 사용자가 “어떤 명령이 핵심 사용 흐름인지” 바로 이해함

---

## T12 — Boot / Restore 경량화
### 목적
cold start 비용을 낮춘다.

### 현재 관찰
- persisted `search_docs` 로부터 메모리 인덱스를 다시 조립한다.
- 기본 embedder 는 로컬 deterministic 이므로 치명적이지는 않지만, corpus 가 커지면 startup 비용이 커질 수 있다.

### 개선 후보
1. 현재 유지 + 측정만
2. lexical stats/vector cache projection 저장
3. 최근 사용 scope 우선 restore
4. lazy restore / on-demand hydrate

### 권장 순서
- **먼저 1**
- 필요 시 **3**
- 정말 필요할 때만 **2**
- 4 는 설계 복잡도가 커질 수 있으므로 신중

---

## 4. 검색 개선 태스크 (P2)

## T20 — SQLite FTS5 Prototype
### 목적
새로운 외부 DB 없이 lexical retrieval 을 강화.

### 설계
- `search_docs` 는 canonical projection 유지
- FTS5 virtual table 은 검색 acceleration layer
- rebuild 가능해야 함

### 예시
```sql
CREATE VIRTUAL TABLE IF NOT EXISTS search_docs_fts
USING fts5(
    uri UNINDEXED,
    name,
    abstract_text,
    content,
    tags_text,
    tokenize = 'unicode61'
);
```

### 동기화 방식
- simplest first:
  - reindex 시 full rebuild
- 다음 단계:
  - upsert/delete 시 dual write

### 검색 흐름
1. exact/path/header boost
2. FTS5 BM25 lexical 후보 추출
3. 현재 in-memory dense/sparse 로 rerank
4. final notes 기록

### 완료 기준
- small/medium corpus 에서 retrieval quality 가 체감상 개선
- second DB 없이도 품질이 상승

---

## T21 — Hybrid score 실험
### 목적
현재 in-memory score 와 FTS5 score 결합.

### 실험 매트릭스
- `score = a*exact + b*fts + c*dense + d*sparse + e*recency + f*path`
- 질의 유형별 가중치 실험
  - exact file/path
  - concept lookup
  - memory/preference
  - session recall

### 완료 기준
- top1 / top3 / MRR / nDCG 개선이 관측됨

---

## T22 — Search Corpus Policy 문서화
### 목적
무엇을 index 하는지 명확히.

### 포함 내용
- 파일 size cap
- truncation rule
- session indexing rule
- OM indexing rule
- excluded/internal scope rule
- tag normalization
- reindex trigger

### 완료 기준
- 검색 결과 편차 원인을 문서로 설명 가능

---

## 5. 중기 태스크 (P3)

## T30 — Sync 요구사항 명세
### 목적
team mode 가 정말 필요한지, 필요하면 어디까지인지 명확화.

### 문서 질문
- 공유해야 하는 scope 는 무엇인가?
- 세션은 공유하는가?
- conflict 는 last-write-wins 인가?
- offline write 는 어떻게 merge 하는가?
- local-only scope 와 shared scope 를 어떻게 나누는가?

### 완료 기준
- sync 가 storage 문제인지 product 문제인지 구분됨

---

## T31 — libSQL/Turso feasibility 검토
### 목적
local-first + remote sync 의 현실성 확인.

### 조건
다음이 모두 충족될 때만:
- 실제 small-team 공유 요구가 존재
- 단일 로컬 워크스페이스를 넘어선 사용 사례가 있음
- 로컬 장애 없이 remote sync 를 원하는 요구가 명확

### 완료 기준
- “도입”이 아니라 “도입해야 하는지”가 명확해짐

---

## 6. 보류 태스크 (현재 하지 않음)

## X01 — Postgres/pgvector 전환
이유:
- 운영 표면 증가
- 현재 타깃과 불일치
- canonical state 복잡도 증가

## X02 — Qdrant canonical 채택
이유:
- 작은 팀/즉시 배포 가치보다 service 운영 비용이 큼

## X03 — sqlite-vec 필수 의존성 고정
이유:
- 실험 가치가 있어도 핵심 기반으로는 아직 이르다

---

## 7. 성능 개선 가이드

## 7.1 측정 순서
1. 재현
2. 관측
3. 가설
4. 검증
5. 근본 수정(RCA)

### 예시 템플릿
```text
증상: medium corpus 에서 cold boot 가 8초 이상
재현: fixtures + synthetic corpus 5만 문서
관측: search_docs 전체 restore 구간이 70%
가설: restore 중 full text/vector 재구성이 병목
검증: profiler + feature flag 로 lexical-only restore 비교
수정: recent-scope eager restore + lazy hydrate
```

## 7.2 SQLite 체크리스트
- WAL 유지
- `PRAGMA foreign_keys = ON` 유지
- hot query `EXPLAIN QUERY PLAN` 캡처
- `busy_timeout` 실험
- 큰 쓰기는 transaction 으로 묶기
- outbox/search/OM hot path index 검토
- vacuum/auto_vacuum 정책은 실제 파일 성장 패턴을 보고 결정

## 7.3 검색 성능 체크리스트
- warm index size 측정
- boot 시 restore 단계별 시간 측정
- exact/path hit 와 semantic hit 비율 분리
- session query 와 resource query latency 분리
- reranker on/off 차이 측정

## 7.4 운영 단순성 체크리스트
다음 질문에 모두 “예”여야 한다.
- `context.db` 하나만 백업하면 되는가?
- 검색 인덱스는 잃어도 복구 가능한가?
- queue 가 꼬여도 reconcile 로 회복 가능한가?
- 외부 서비스 없이 개발/테스트 가능한가?

---

## 8. 작업 백로그

| ID | 작업 | 우선순위 | 난이도 | 비고 |
|---|---|---:|---:|---|
| T01 | Ownership Map 작성 | P0 | S | 바로 시작 |
| T02 | 성능 기준선 측정 | P0 | M | 추측 제거 |
| T03 | SQLite access pattern 리뷰 | P0 | M | queue/search hot path |
| T04 | Query plan telemetry 정리 | P0 | S | 디버깅 가치 큼 |
| T10 | core/ops 경계 문서화 | P1 | M | 구조 안정화 |
| T11 | CLI surface 재구성 | P1 | M | UX 개선 |
| T12 | boot/restore 경량화 | P1 | M-L | 측정 후 |
| T20 | FTS5 prototype | P2 | M | 가장 가치 큼 |
| T21 | hybrid score 실험 | P2 | M | relevance 향상 |
| T22 | search corpus policy 문서화 | P2 | S | 재현성 |
| T30 | sync 요구사항 명세 | P3 | S | product decision |
| T31 | libSQL/Turso feasibility | P3 | M | 실제 필요 시 |
| X01 | Postgres/pgvector 전환 | 보류 | L | 지금 아님 |
| X02 | Qdrant canonical 채택 | 보류 | L | 지금 아님 |
| X03 | sqlite-vec 필수화 | 보류 | M | 실험용만 |

---

## 9. 30일 실행안

### Week 1
- T01 Ownership Map
- T02 baseline benchmark
- T04 telemetry 정리

### Week 2
- T03 SQL hot path 리뷰
- outbox composite index 실험
- restore profiling

### Week 3
- T20 FTS5 prototype
- lexical candidate + rerank 연결

### Week 4
- T21 score tuning
- T22 corpus/indexing policy 문서화
- Phase review: “SQLite 유지 + FTS5/hybrid 로 충분한가?”

---

## 10. 최종 판단 문장

> **AxiomSync 의 다음 단계는 “더 큰 데이터베이스로 도망가는 것”이 아니라, “지금의 로컬-퍼스트 구조를 더 날카롭게 만드는 것”이다.**

---

## 11. 참고 소스

- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/docs/ARCHITECTURE.md
- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/docs/API_CONTRACT.md
- https://raw.githubusercontent.com/AxiomOrient/AxiomSync/main/crates/axiomsync/TEST_INTENT.md
- https://sqlite.org/wal.html
- https://sqlite.org/fts5.html
- https://github.com/pgvector/pgvector
- https://qdrant.tech/documentation/quickstart/
- https://docs.lancedb.com/quickstart
- https://docs.turso.tech/features/embedded-replicas/introduction
- https://github.com/asg017/sqlite-vec
