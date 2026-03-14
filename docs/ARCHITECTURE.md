# AxiomSync Architecture

핵심 구조는 간단합니다. `axiom://` URI, 단일 `context.db`, 메모리 검색 런타임, 명시적인 세션/OM 상태입니다.

## Repository Boundary
- Inside this repository: runtime library, operator CLI, release scripts
- Outside this repository: web companion, mobile FFI companion, app-specific frontend shells

## Layers
- Interface: CLI parses commands and delegates to runtime
- Facade: `AxiomSync` coordinates filesystem, state, retrieval, session, release
- Storage: `LocalContextFs` + `SqliteStateStore`
- Retrieval: `search_docs` + `search_docs_fts` persisted state, memory-only runtime query path
- Session and OM: explicit session state + vendored OM engine under `src/om/engine`
- Release and Evidence: benchmark, eval, security, operability, contract gates

## Main Data Flows
- Bootstrap: filesystem scopes 생성, runtime state restore
- Ingest and Replay: resource ingest, queue write, SQLite projection update
- Query: memory-only retrieval, trace 기록
- Session and OM: session memory update, restart-safe checkpoint/replay
- Release: executable gate 실행

## Boundary Rules
- Side effects belong at filesystem and state boundaries, not inside pure selection logic.
- Startup is a hard cutover to `context.db`; legacy DB discovery and migration are out of scope.
- In-place schema repair inside `context.db` is allowed only for known compatibility cleanup.
- Retrieval backend policy is `memory_only`; `sqlite` retrieval mode is rejected as configuration error.
- `queue` scope is system-owned for writes.
- Vendored OM code remains explicit under `src/om/engine`; runtime-only policy stays in `axiomsync::om`.
