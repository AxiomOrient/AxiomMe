# AxiomMe

AxiomMe is a local-first context runtime for building deterministic agent systems.
It treats data as the primary concern: explicit URI-addressed content, explicit queue/state
transitions, and explicit retrieval/memory workflows.

This README explains the project itself. Detailed API and policy contracts live in `docs/`.

## What Problem This Project Solves

Most agent runtimes hide state transitions behind framework abstractions. That makes behavior hard
to debug, tune, and trust in production.

AxiomMe focuses on:
- deterministic local execution by default
- visible and queryable data model
- recoverable queue/state lifecycle
- measurable quality and release gates

## Core Ideas

### 1) Data Is Primary

All content is addressed with a canonical URI:

`axiom://{scope}/{path}`

Scopes:
- External scopes: `resources`, `user`, `agent`, `session`
- Internal scopes: `temp`, `queue`

The URI model is the center of the system. Filesystem layout, indexing, queue payloads, and
retrieval boundaries all follow this model.

### 2) Pure Transformations, Explicit Effects

- Pure logic lives in parser/retrieval/OM transform modules.
- Side effects are isolated in explicit services:
  - filesystem writes
  - sqlite persistence
  - queue replay
  - network calls

### 3) Mechanical Sympathy

The runtime is designed to make cost visible:
- queue statuses are explicit (`new`, `processing`, `done`, `dead_letter`)
- retries use deterministic backoff
- retrieval/index behavior is measurable and testable
- release gates enforce operational quality before shipping

## Architecture Overview

The workspace is split by responsibility:

- `crates/axiomme-core`
  - runtime engine and domain model
  - URI/scopes, fs safety, sqlite state, in-memory index
  - ingest, replay/reconcile, retrieval, session memory, release evidence
- `crates/axiomme-cli`
  - command surface and process contracts
- `crates/axiomme-web`
  - local web editor/API on top of `axiomme-core`

Canonical spec and contract docs:
- `docs/README.md`
- `docs/FEATURE_SPEC.md`
- `docs/API_CONTRACT.md`

## Runtime Model

### Lifecycle

1. `AxiomMe::new(root)`
2. `bootstrap()`
3. `prepare_runtime()`
4. `initialize()` (runtime-ready alias)

### Durable vs Ephemeral State

- Durable state:
  - scoped files in workspace
  - sqlite state store (`.axiomme_state.sqlite3`)
- Ephemeral state:
  - in-memory search index
  - request-scoped runtime hints

### Queue and Recovery

Queue events are stored durably and replayed explicitly. Replay/reconcile flows are first-class.
This allows predictable recovery after process restarts or partial failures.

## What You Can Build With AxiomMe

- local and remote resource ingestion pipelines
- searchable context corpora with traceable retrieval
- session memory extraction and promotion workflows
- reliability/operability/security evidence generation
- release gating workflow for production readiness
- local web-based document/markdown editing with safe defaults

## Repository Map

- `crates/axiomme-core/src/client/*`: orchestration services
- `crates/axiomme-core/src/state/*`: sqlite schema and persistence
- `crates/axiomme-core/src/retrieval/*`: planner/engine/scoring
- `crates/axiomme-core/src/session/*`: session and memory lifecycle
- `crates/axiomme-core/src/om/*`: OM transforms, parsing, and pipeline
- `crates/axiomme-cli/src/*`: CLI arguments and command handlers
- `crates/axiomme-web/src/*`: HTTP handlers, DTOs, security, UI assets

## Quickstart

```bash
# 1) Inspect commands
cargo run -p axiomme-cli -- --help

# 2) Initialize runtime in current workspace
cargo run -p axiomme-cli -- init

# 3) Ingest content
cargo run -p axiomme-cli -- add ./README.md --target axiom://resources/repo --wait true

# 4) Retrieve context
cargo run -p axiomme-cli -- find "context runtime"
```

Run web editor:

```bash
cargo run -p axiomme-cli -- web --host 127.0.0.1 --port 8787
```

## Development Workflow

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Convenience script:

```bash
bash scripts/quality_gates.sh
```

## Release Workflow

Run release gate pack locally:

```bash
cargo run -p axiomme-cli -- release pack --workspace-dir .
```

Gate pack evaluates the full readiness sequence (`G0..G8`) and writes an evidence report
under `axiom://queue/release/packs/`.

## Configuration (High-Level)

Important environment families:
- retrieval backend policy
- embedding provider/runtime
- indexing and reranking behavior

For exact variable names and semantics, use:
- `docs/API_CONTRACT.md`
- `docs/FEATURE_SPEC.md`

## Project Principles

- Data model first, URI model explicit
- Pure transforms where possible
- Side effects isolated and auditable
- No accidental abstraction
- Performance characteristics made explicit
- Operational correctness verified by tests and gates
