# AxiomSync

Local-first context runtime and operator CLI for agentic systems.

AxiomSync는 `axiom://` URI, 단일 SQLite 상태 저장소, 메모리 검색 인덱스, 세션/OM 메모리 흐름을 하나의 로컬 런타임 계약으로 묶습니다. 이 저장소는 런타임과 CLI만 소유합니다.

## Release Line
- Current repository release line: `v1.0.1`
- Canonical local store: `<root>/context.db`
- Retrieval policy: `memory_only`
- Persistence policy: SQLite only

## Repository Boundary
In this repository:
- `crates/axiomsync`: runtime library and operator CLI binary
- `docs/`: architecture and contract documentation
- `scripts/`: quality and release gate entrypoints

Outside this repository:
- web companion project
- mobile FFI companion project
- app-specific frontend shells

## Quick Start
```bash
cargo run -p axiomsync -- --help

cargo run -p axiomsync -- init
cargo run -p axiomsync -- add ./docs --target axiom://resources/docs
cargo run -p axiomsync -- search "oauth flow"
cargo run -p axiomsync -- session commit
```

## Runtime Model
- URI model: `axiom://{scope}/{path}`
- State store: `context.db` stores queue, checkpoints, OM state, and persisted search state
- Query path: runtime restores an in-memory index from persisted search state and executes `find/search`
- Session/OM path: observer and reflector flows update explicit session memory state
- Release path: contract, reliability, eval, security, benchmark, and operability gates are executable

## Documentation Map
- [docs/README.md](./docs/README.md): documentation entrypoint
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md): runtime layers and data flow
- [docs/API_CONTRACT.md](./docs/API_CONTRACT.md): stable runtime and release contracts
- [crates/README.md](./crates/README.md): package map
- [crates/axiomsync/README.md](./crates/axiomsync/README.md): runtime and CLI package boundary

## Quality And Release
```bash
bash scripts/quality_gates.sh
bash scripts/release_pack_strict_gate.sh --workspace-dir "$(pwd)"
```

## Non-Negotiable Rules
- Canonical URI protocol stays `axiom://`
- Runtime startup is a hard cutover to `context.db`
- Legacy DB filename discovery or migration is not supported
- Retrieval backend remains `memory_only`; `sqlite` retrieval mode is rejected
- Vendored pure-OM boundary remains explicit under `axiomsync::om`
