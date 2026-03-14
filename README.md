# AxiomSync

Local-first context runtime and operator CLI for agentic systems.

AxiomSync는 `axiom://` URI, `context.db`, 메모리 검색 런타임, 세션/OM 상태를 하나로 묶는 로컬 런타임입니다. 이 저장소는 런타임과 CLI만 소유합니다.

## Release Line
- Current repository release line: `v1.2.0`
- Canonical local store: `<root>/context.db`
- Retrieval policy: `memory_only`
- Persistence policy: SQLite only

## Repository Boundary
- In this repository: `crates/axiomsync`, `docs/`, `scripts/`
- Outside this repository: web companion, mobile FFI companion, app-specific frontend shells

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
- State store: `context.db`
- Retrieval runtime: `memory_only`
- Persisted retrieval state: `search_docs` + `search_docs_fts`
- Canonical result shape: `FindResult.query_results` + `hit_buckets`
- Compatibility result shape: `memories/resources/skills`
- Session/OM state: explicit and durable

## Documentation Map
- [docs/README.md](./docs/README.md): documentation entrypoint
- [docs/API_CONTRACT.md](./docs/API_CONTRACT.md): stable contract
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md): runtime structure
- [docs/RETRIEVAL_STACK.md](./docs/RETRIEVAL_STACK.md): retrieval path
- [docs/OWNERSHIP_MAP.md](./docs/OWNERSHIP_MAP.md): change routing
- [plans/README.md](./plans/README.md): roadmap, task tracker, and rollout rationale

## Quality And Release
```bash
bash scripts/quality_gates.sh
bash scripts/release_pack_strict_gate.sh --workspace-dir "$(pwd)"
```

## Non-Negotiable Rules
- Canonical URI protocol stays `axiom://`
- Runtime startup is a hard cutover to `context.db`
- Legacy DB filename discovery or migration is not supported
- Known in-place compatibility repair stays inside `context.db` only
- Retrieval backend remains `memory_only`; `sqlite` retrieval mode is rejected
- Vendored pure-OM boundary remains explicit under `axiomsync::om`
