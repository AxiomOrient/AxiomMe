# Crates

Minimal crate index for AxiomMe.

## Modules (MECE)

- [`axiomme-core`](./axiomme-core/README.md): domain engine, state, retrieval.
- [`axiomme-cli`](./axiomme-cli/README.md): CLI surface.
- [`axiomme-web`](./axiomme-web/README.md): HTTP/UI surface.

OM core is integrated directly in `axiomme-core` (`src/om/*`).

## How To Run (Operator)

```bash
cargo run -p axiomme-cli -- --help
```

### With process-compose

```bash
process-compose --log-file logs/process-compose.log -f process-compose.yaml up
```

- `web`: serves UI/API on `127.0.0.1:8787`
- `queue_daemon`: keeps queue replay worker running for background processing
- runtime logs:
  - `logs/process-compose.log`
  - `logs/web.log`
  - `logs/queue_daemon.log`

## How To Extend (Developer)

1. Add or modify code in the target crate.
2. Run:
```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
