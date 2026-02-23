# axiomme-core

`axiomme-core` is the runtime/data engine for AxiomMe.
It owns one problem end-to-end: **local data processing for indexed retrieval**.

## What This Crate Owns

- Axiom URI model and scope boundaries (`axiom://resources`, `user`, `agent`, `session`, `temp`, `queue`)
- Local context filesystem abstraction with root-boundary safety checks
- SQLite state store (outbox/queue, search docs, index state, traces, checkpoints)
- In-memory index + retrieval orchestration
- Ingest, reindex, queue replay/reconcile, markdown/json/yaml editor paths
- Session memory extraction and release/eval/benchmark evidence pipelines
- OM pure model/transform contracts via `episodic` (wired through `axiomme-core::om`)

## Runtime Lifecycle

- `AxiomMe::new(root)`: construct services and open state DB
- `bootstrap()`: filesystem scopes + required infra bootstrap
- `prepare_runtime()`: bootstrap + tier synthesis + runtime index init
- `initialize()`: alias of `prepare_runtime()` (runtime-ready entrypoint)

## Core Data Flow

1. `add_resource(...)` stages source into temp ingest session
2. finalize into target URI tree
3. enqueue semantic/reindex events into outbox
4. `replay_outbox(...)` processes events
5. `reindex_uri_tree(...)` parses files, updates SQLite search docs/index state, updates memory index
6. retrieval (`find/search`) executes DRR + backend merge/rerank

## Safety/Correctness Invariants

- Filesystem operations enforce root-boundary protections (no path escape).
- `queue` scope is read-only for non-system writes.
- Generated tier files (`.abstract/.overview`) are treated as system artifacts.
- Markdown parser keeps source text content (including frontmatter/metadata blocks) instead of auto-stripping sections.
  - Rationale: avoid heuristic data loss from `---` delimiter collisions with valid markdown content.
  - If metadata exclusion is needed, preprocess content explicitly before ingestion.
- Runtime SQLite state DB (`.axiomme_state.sqlite3` + WAL/SHM) is permission-hardened to owner-only on Unix (`0600`).
- Host-command execution (`cargo`, `sysctl`, etc.) is policy-gated:
  - `AXIOMME_HOST_TOOLS=on|off` override
  - target default: enabled on non-iOS, disabled on iOS
- Reindex and benchmark corpus metadata now **skip symlink entries** to avoid:
  - accidental external file indexing
  - flaky failures on broken links
  - corpus digest drift from non-owned paths

## Key Modules

- `src/client.rs`: public runtime entrypoint (`AxiomMe`)
- `src/config/mod.rs`: runtime config SSOT snapshot (`AppConfig`) loaded once at startup
- `src/client/*`: application services (resource, indexing, queue, search, release, trace)
- `src/fs.rs`: local context filesystem and scope guardrails
- `src/state/*`: SQLite schema, migrations, queue/search persistence
- `src/retrieval/*`: DRR retrieval engine and trace model
- `src/embedding.rs`: embedder selection and strict-error handling
- `src/session/*`: session logs and memory extraction/indexing

Optional feature modules:

- `host-tools` feature enables host process execution boundaries (`cargo`, `sysctl`, etc.).
  - Enabled by default for desktop/CLI builds.
  - Mobile FFI builds should keep this disabled.
- `markdown-preview` feature enables `src/markdown_preview.rs` (pure markdown->safe-html transform).
- Cost boundary is explicit: mobile/native consumers can keep this feature disabled.

## Validation

```bash
cargo clippy -p axiomme-core --all-targets -- -D warnings
cargo test -p axiomme-core
```

Current baseline: `cargo test -p axiomme-core` passes in this repository state.

## Test Intent

Pseudo-code level test intent and coverage map:

- [`TEST_INTENT.md`](./TEST_INTENT.md)

## Extension Rule Of Thumb

- Keep logic close to data ownership.
- Prefer deterministic transforms over hidden side effects.
- Add or update tests for every behavior change at the service boundary.
