# AxiomMe

Rust workspace for a local-first context runtime.

## Workspace

- `crates/axiomme-core`: core runtime, indexing, retrieval, queue/state
- `crates/axiomme-cli`: CLI surface
- `crates/axiomme-web`: local web editor/API

## Quickstart

```bash
cargo run -p axiomme-cli -- --help
```

## Quality Gates

```bash
bash scripts/quality_gates.sh
```

## Retrieval Backend

```bash
export AXIOMME_RETRIEVAL_BACKEND=sqlite
# or:
export AXIOMME_RETRIEVAL_BACKEND=memory
```
