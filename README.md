# AxiomMe

AxiomMe is a local-first context runtime for deterministic agent workflows.

This repository is intentionally focused on three boundaries only:
- `axiomme-core`: data model + runtime engine
- `axiomme-cli`: explicit process/automation interface
- `axiomme-mobile-ffi`: native FFI boundary on top of core

Web UI delivery is intentionally externalized (see "Companion Projects").

## Repository Scope

Included in this repository:
- `crates/axiomme-core`
- `crates/axiomme-cli`
- `crates/axiomme-mobile-ffi`

Not included in this repository:
- Web viewer/server runtime (separate project)
- iOS/Android application projects (separate project)

This keeps runtime correctness and packaging boundaries explicit.

## Data Model (Primary)

All content is addressed by canonical URI:

`axiom://{scope}/{path}`

Scopes:
- external: `resources`, `user`, `agent`, `session`
- internal: `temp`, `queue`

Everything else derives from this model:
- filesystem placement
- indexing/retrieval keys
- queue payload routing
- session/memory references

## Runtime Boundaries

`axiomme-core`:
- URI model, fs safety, queue/state, retrieval, session memory, release evidence.
- OM model/transform engine is provided by `episodic` and integrated via `axiomme-core::om`.
- Pure transformations are preferred; side effects are isolated in explicit modules.

`axiomme-cli`:
- deterministic command surface for operators/CI.
- explicit external handoff for viewer command (`axiomme web`).

`axiomme-mobile-ffi`:
- `staticlib`/`cdylib` exports for native mobile integration.
- JSON C-ABI contracts and explicit runtime handle ownership.

## Side Effects (Explicit)

Main side-effect boundaries in core:
- filesystem read/write
- sqlite persistence
- queue replay and reconciliation
- network calls (embedding, remote resources)
- optional host tool execution (policy-gated)

Host command policy:
- `AXIOMME_HOST_TOOLS=on|off`
- default is target-driven (`on` for non-iOS targets, `off` for iOS targets)
- compile-time boundary: `axiomme-core` `host-tools` feature (enabled for CLI, disabled in mobile FFI crate)

## What You Can Build

- deterministic local ingestion + retrieval pipelines
- searchable context corpora with traceable query plans
- session memory extraction/promotion workflows
- reliability/operability/security evidence pipelines
- release gate automation (`G0..G8`)
  - `G0` enforces `episodic` API probe + semver/registry contract in addition to core contract probe
- native mobile clients via FFI boundary crate

## Quickstart

```bash
# inspect CLI surface
cargo run -p axiomme-cli -- --help

# initialize runtime in current workspace
cargo run -p axiomme-cli -- init

# ingest content
cargo run -p axiomme-cli -- add ./README.md --target axiom://resources/repo --wait true

# retrieval
cargo run -p axiomme-cli -- find "context runtime"
```

Web viewer handoff:

```bash
cargo run -p axiomme-cli -- web --host 127.0.0.1 --port 8787
```

Viewer binary resolution order:
- `AXIOMME_WEB_VIEWER_BIN`
- `axiomme-webd`

## Development and Verification

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Convenience script:

```bash
bash scripts/quality_gates.sh
```

Real-use random benchmark (contextSet):

```bash
bash scripts/contextset_random_benchmark.sh \
  --dataset /Users/axient/Documents/contextSet \
  --sample-size 24 \
  --seed 4242 \
  --report-path docs/REAL_CONTEXTSET_VALIDATION_$(date +%F)-random.md

# Multi-seed matrix benchmark (recommended gate)
bash scripts/contextset_random_benchmark_matrix.sh \
  --dataset /Users/axient/Documents/contextSet \
  --sample-size 24 \
  --seeds 4242,777,9001 \
  --report-path docs/REAL_CONTEXTSET_VALIDATION_MATRIX_$(date +%F).md

# Default gate includes:
# find/search non-empty, find/search top1 (>=65), find/search top5, p95 latency reporting.
# Heading candidates exclude YAML front matter and fenced code blocks.
```

Release gate pack:

```bash
cargo run -p axiomme-cli -- release pack --workspace-dir .
```

Final release signoff:

```bash
# record final decision (GO or NO-GO)
scripts/record_release_signoff.sh --decision GO --name <release-owner>

# regenerate deterministic readiness snapshot
scripts/release_signoff_status.sh --report-path docs/RELEASE_SIGNOFF_STATUS_$(date -u +%F).md
```

Release publish precondition:
- `docs/RELEASE_SIGNOFF_STATUS_YYYY-MM-DD.md` shows `Overall: READY`
- `Final Release Decision` starts with `DONE`

## Companion Projects

Recommended split under `/Users/axient/repository`:
- `/Users/axient/repository/AxiomMe-web`
  - viewer/server delivery only
  - depends on stable runtime/API contracts
- `/Users/axient/repository/AxiomMe-mobile`
  - iOS/Android app projects
  - consumes `axiomme-mobile-ffi`

This repository stays runtime-focused so release risk and performance behavior stay easy to reason about.

## Documentation

Canonical contracts/specs:
- `docs/README.md`
- `docs/FEATURE_SPEC.md`
- `docs/API_CONTRACT.md`

Web split decision and migration notes:
- `docs/WEB_SPLIT_REVIEW_2026-02-21.md`

## Principles

- data model first
- pure transforms where possible
- explicit side effects
- concrete structures over clever hierarchies
- predictable performance and ownership boundaries
