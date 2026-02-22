# AxiomMe

Local-first context runtime for deterministic retrieval, explicit memory evolution, and
release-grade operability checks.

The repository is intentionally concrete:
- data model first (`axiom://` URI + scoped filesystem + SQLite state + in-memory index)
- pure transforms where possible (`om/`, retrieval planner/scoring)
- side effects isolated to explicit service boundaries (`client/*`, `state/*`, `fs`, `pack`)

## What You Can Build With It

- Ingest local files/dirs or HTTP(S) text into URI-scoped context trees.
- Search and retrieve context with trace artifacts (`find` / `search`).
- Run session memory extraction and explicit memory promotion/checkpoint flows.
- Recover from drift/failure via queue replay and reconcile.
- Export/import context trees safely as ovpack archives.
- Serve a local web editor for markdown/document/fs operations with security guards.
- Produce release gate evidence (`G0..G8`) and enforce CI-quality constraints.

## Workspace Layout

- `crates/axiomme-core`
  - runtime and domain engine
  - filesystem model, URI model, queue/state, indexing/retrieval
  - session/OM flows and release/evidence pipelines
- `crates/axiomme-cli`
  - command surface and process contracts
- `crates/axiomme-web`
  - local web server/editor and HTTP API
- `docs`
  - canonical spec/contract set

## Data Model (Primary)

### URI and Scope

All content is addressed by:

`axiom://{scope}/{path}`

Scopes:
- external: `resources`, `user`, `agent`, `session`
- internal: `temp`, `queue`

Hard invariants:
- traversal and root escape are rejected
- internal scope writes are restricted
- `queue` is read-only for non-system mutations

### Durable and Ephemeral State

- durable:
  - scoped files under workspace root
  - `.axiomme_state.sqlite3` for queue/index/search/trace/checkpoint metadata
- ephemeral:
  - in-memory search index (`InMemoryIndex`)
  - request-scoped runtime hints for search

### Queue Semantics

Outbox rows have explicit lifecycle states:
- `new`
- `processing`
- `done`
- `dead_letter`

Runtime behavior:
- replay consumes `new` due events
- stale `processing` rows are explicitly recovered back to `new` before replay
- retry/backoff policy is deterministic and bounded

## Runtime Lifecycle

1. `AxiomMe::new(root)`
2. `bootstrap()`
3. `prepare_runtime()`
4. `initialize()` (runtime-ready alias of `prepare_runtime`)

`prepare_runtime()` does only three things:
- ensure scope tiers
- initialize or restore index state
- validate runtime config invariants

## Main Execution Flows

### Ingest and Index

1. `add_resource(...)`
2. stage in `temp`
3. finalize to target URI tree
4. enqueue semantic scan/reindex events
5. `replay_outbox(...)`
6. `reindex_uri_tree(...)`

### Retrieval

- `find`: deterministic memory-backend retrieval and trace output
- `search`: same retrieval path + session/OM hint composition

Both return:
- ranked hits
- query plan notes
- trace metadata URI

### Session and Memory

- session append -> commit
- archive active messages
- extract categorized memories
- optional explicit promotion checkpoint flow for durable writes

## Quickstart

```bash
# 1) show CLI surface
cargo run -p axiomme-cli -- --help

# 2) initialize runtime at current workspace
cargo run -p axiomme-cli -- init

# 3) ingest and process
cargo run -p axiomme-cli -- add ./README.md --target axiom://resources/repo --wait true

# 4) retrieve
cargo run -p axiomme-cli -- find "release gate"
```

## Web Editor

```bash
cargo run -p axiomme-cli -- web --host 127.0.0.1 --port 8787
```

Behavioral notes:
- startup performs scoped reconcile before serving
- markdown/document saves are full-replace only
- etag conflicts return `409`
- in-flight save lock returns `423`
- preview sanitizes unsafe HTML and URL schemes

## Configuration (Key Environment Variables)

- retrieval:
  - `AXIOMME_RETRIEVAL_BACKEND=memory`
  - invalid backend token fails fast
- embedding:
  - `AXIOMME_EMBEDDER=semantic-lite|hash|semantic-model-http`
  - `AXIOMME_EMBEDDER_MODEL_ENDPOINT`
  - `AXIOMME_EMBEDDER_MODEL_NAME`
  - `AXIOMME_EMBEDDER_MODEL_TIMEOUT_MS`
  - `AXIOMME_EMBEDDER_STRICT`
- indexing/search behavior:
  - `AXIOMME_TIER_SYNTHESIS`
  - `AXIOMME_RERANKER`

## Development Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Repository convenience script:

```bash
bash scripts/quality_gates.sh
```

## Release Workflow

For local release-grade validation:

```bash
cargo run -p axiomme-cli -- release pack --workspace-dir .
```

The pack evaluates gates `G0..G8` and writes a report under:

- `axiom://queue/release/packs/{pack_id}.json`

Gate focus:
- contract integrity
- build quality
- reliability evidence
- eval quality
- session memory quality
- security audit
- benchmark gate
- operability evidence
- blocker rollup

## Design Principles

- Keep data visible and explicit.
- Prefer pure transforms and deterministic behavior.
- Isolate effects (filesystem, sqlite, network, subprocess).
- Make performance costs obvious (lock scope, allocations, queue/backoff policy).
- Avoid accidental abstraction and hidden indirection.

## Canonical Docs

- `docs/README.md`
- `docs/FEATURE_SPEC.md`
- `docs/API_CONTRACT.md`
