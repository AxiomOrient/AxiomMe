# AxiomSync Architecture

이 저장소는 로컬 우선 컨텍스트 런타임과 그 위의 operator CLI를 소유합니다. 핵심 설계는 단순합니다: `axiom://` URI 모델, 단일 `context.db`, 부팅 시 복원되는 메모리 검색 인덱스, 그리고 명시적인 세션/OM 상태 전이입니다.

## Repository Boundary
Inside this repository:
- runtime library: `crates/axiomsync`
- operator CLI binary: `crates/axiomsync/src/main.rs`
- release and quality gate scripts: `scripts/`

Outside this repository:
- web companion project
- mobile FFI companion project
- app-specific frontend shells

## Core Runtime Model
- `AxiomUri`: `axiom://{scope}/{path}` canonical identifier
- `Scope`: `resources|user|agent|session|temp|queue`
- `context.db`: queue, checkpoints, OM state, persisted search state
- `InMemoryIndex`: runtime retrieval projection restored from persisted state
- `OmRecord` and observation entries: explicit session/OM memory units
- Trace and release evidence artifacts: searchable operational outputs under `queue`

## Layers
1. Interface
- `axiomsync` binary parses operator commands and delegates to the runtime

2. Facade
- `AxiomSync` coordinates filesystem, state store, retrieval, session, and release services

3. Storage
- `LocalContextFs` owns rooted filesystem access and scope rules
- `SqliteStateStore` owns durable runtime state inside `context.db`

4. Retrieval
- indexing services parse staged resources
- persisted search state is updated in SQLite
- runtime restores a memory index and executes `find/search`

5. Session And OM
- session services manage logs, extraction, promotion, and checkpoints
- vendored OM contract and transform code lives under `src/om/engine`
- runtime-facing OM boundary is exposed through `axiomsync::om`

6. Release And Evidence
- benchmark, eval, security audit, reliability, operability, and contract gates run as executable checks
- CLI `release pack` composes those gates into one release verdict

## Main Data Flows
1. Bootstrap
- `bootstrap()` creates filesystem scopes and required runtime infrastructure
- `prepare_runtime()` restores runtime state and retrieval services

2. Ingest And Replay
- `add_resource(...)` stages content into the ingest path
- queue events are written to SQLite
- replay workers update persisted search state and runtime index state

3. Query
- `find/search` start from rooted URI scopes
- retrieval uses the memory backend only
- traces record query plan, explored nodes, and convergence metrics

4. Session And OM
- observer and reflector flows consume session messages
- derived memory state is persisted explicitly
- replay and checkpoint operations remain restart-safe

5. Release
- release probes validate contract integrity, build quality, reliability, eval quality, session memory, security audit, benchmark, and operability

## Boundary Rules
- Side effects belong at filesystem and state boundaries, not inside pure selection logic.
- Startup is a hard cutover to `context.db`; legacy DB discovery and migration are out of scope.
- Retrieval backend policy is `memory_only`; `sqlite` retrieval mode is rejected as configuration error.
- `queue` scope is system-owned for writes.
- Vendored OM code remains explicit under `src/om/engine`; runtime-only policy stays in `axiomsync::om`.
