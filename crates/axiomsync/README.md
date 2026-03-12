# axiomsync

`axiomsync`는 AxiomSync의 로컬 런타임과 operator CLI binary를 함께 담는 단일 crate입니다. 이 crate는 URI 모델, rooted filesystem, `context.db`, 검색 런타임, 세션/OM 메모리, 그리고 release evidence 흐름을 끝까지 소유합니다.

## Ownership
- `axiom://` URI model and scope boundaries
- rooted local filesystem abstraction
- SQLite persistence in `context.db`
- persisted search state plus restored in-memory retrieval index
- ingest, replay, reindex, trace, eval, benchmark, and release evidence services
- vendored OM contract and transform engine under `src/om/engine`
- operator CLI entrypoint under `src/main.rs`, `src/cli/*`, `src/commands/*`

## Lifecycle
- `AxiomSync::new(root)`: runtime service graph 구성
- `bootstrap()`: scope directories와 기본 인프라 생성
- `prepare_runtime()`: bootstrap + tier synthesis + runtime index restore
- `initialize()`: runtime-ready entrypoint

## Important Invariants
- Runtime startup is a hard cutover to `context.db`.
- Legacy DB discovery and migration are not supported.
- Retrieval backend is `memory_only`.
- `queue` scope is system-owned for writes.
- Filesystem operations enforce rooted path boundaries.
- Runtime DB permissions are hardened to owner-only on Unix.
- External mobile/native consumers should keep host-tool usage explicit.

## Module Map
- `src/client.rs`: public facade
- `src/client/*`: application services
- `src/fs.rs`: rooted filesystem rules
- `src/state/*`: SQLite persistence
- `src/retrieval/*`: retrieval engine and traces
- `src/session/*`: session and memory flows
- `src/om/*`: runtime-facing OM boundary and vendored engine
- `src/release_gate/*`: executable release contract checks

## Features
- `host-tools`: host command execution boundaries
- `markdown-preview`: markdown to safe HTML transform

## Verification
```bash
cargo run -p axiomsync -- --help
cargo clippy -p axiomsync --all-targets -- -D warnings
cargo test -p axiomsync
```

## Test Intent
- [`TEST_INTENT.md`](./TEST_INTENT.md)
