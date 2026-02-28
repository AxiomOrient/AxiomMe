# AxiomMe Architecture Specification

## 1. System Intent
AxiomMe는 로컬 환경에 최적화된 에이전트 컨텍스트 관리 시스템입니다. `axiom://` 가상 URI 스키마를 통해 비정형 데이터(Markdown), 정형 상태(SQLite), 고속 검색 인덱스(In-memory)를 단일 인터페이스로 통합합니다.

## 2. Core Data Model
시스템의 모든 연산은 데이터 중심(Data-first)으로 설계되었습니다.
- **AxiomUri**: `axiom://{scope}/{path}` 형식의 유일 식별자. 스코프(`user`, `agent`, `resources`, `session`, `temp`, `queue`)에 따라 데이터의 생명주기와 접근 권한을 결정하는 시스템의 핵심 불변값입니다.
- **Scope**: 물리적 저장소 및 논리적 격리 단위. 
- **ContextHit**: 검색 엔진이 반환하는 통합 결과 데이터 구조.

## 3. Architecture Layers

### 3.1 Interface Layer
- **axiomme-cli**: 사용자와 시스템 간의 터미널 기반 인터페이스. 명령어를 코어 API로 매핑합니다.
- **axiomme-mobile-ffi**: C ABI를 통한 플랫폼 바인딩. 메모리 소유권을 명시적으로 관리하여 외부 런타임에 기능을 노출합니다.

### 3.2 Coordination Layer (AxiomMe Facade)
- 시스템의 진입점으로서 각 서브 시스템(Fs, State, Index, Retrieval)의 생명주기를 오케스트레이션합니다.

### 3.3 Logic & Storage Layer
- **Virtual File System (LocalContextFs)**: 물리적 디스크를 `axiom://` 경로로 가상화합니다. 모든 파일 I/O 부수 효과는 이 모듈에 격리됩니다.
- **Persistent State (SqliteStateStore)**: 큐(Queue) 처리, 비동기 아웃박스, 세션 체크포인트의 원자성(Atomicity)을 보장합니다.
- **In-memory Index (InMemoryIndex)**: 고성능 검색을 위해 메타데이터와 텍스트를 메모리에 적재합니다. `RwLock`을 통해 명시적으로 동시성을 제어합니다.
- **Retrieval Engine (DrrEngine)**: 핫 루프(Hot-loop) 최적화가 적용된 Document Retrieval and Ranking 엔진입니다. 파싱 비용을 최소화하기 위해 문자열 프리픽스 기반 매칭을 수행합니다.
- **Session Manager**: 세션 상태 및 기억 승격(Memory Promotion)을 담당합니다. LLM을 통한 중복 제거와 체크포인트 기반의 상태 복구를 관리합니다.

## 4. Operational Data Flow
1. **Ingest**: `add_resource` 호출 시 데이터가 가상 FS에 저장되고, 관련 이벤트가 SQLite 큐에 적재됩니다.
2. **Replay**: 비동기 워커가 큐를 처리하며 인메모리 인덱스를 최신 상태로 동기화합니다.
3. **Query**: `find/search` 요청 시 DRR 엔진이 인메모리 인덱스를 탐색하며, 최적화된 수렴 알고리즘을 통해 랭킹 결과를 반환합니다.
4. **Commit**: 세션 종료 시 대화 이력이 아카이브되고, 핵심 지식이 추출되어 사용자/에이전트 메모리로 승격됩니다.

## 5. Performance Design (Carmack Principles)
- **Mechanical Sympathy**: 검색 루프 내의 동적 할당을 최소화하고, 무거운 URI 파싱 대신 명시적인 문자열 조작을 사용합니다.
- **Explicit Control**: 부수 효과가 발생하는 지점(I/O, State Mutation)을 명확히 정의하여 성능 특성을 예측 가능하게 합니다.
- **Memory Efficiency**: `with_capacity`를 통한 재할당 억제 및 `Arc<str>` 기반의 문자열 공유를 활용합니다.
