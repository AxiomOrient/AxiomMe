# Crates

Minimal crate index for the runtime repository.

## Modules

- [`axiomme-core`](./axiomme-core/README.md): domain/runtime engine, persistence, retrieval.
- [`axiomme-cli`](./axiomme-cli/README.md): operator/automation command surface.
- [`axiomme-mobile-ffi`](./axiomme-mobile-ffi/README.md): native mobile FFI boundary.

Out of scope in this repository:
- web viewer/server crate (moved to external project)
- iOS/Android app projects

## Run

```bash
cargo run -p axiomme-cli -- --help
```

Queue daemon (local operator workflow):

```bash
process-compose --log-file logs/process-compose.log -f process-compose.yaml up
```

Logs:
- `logs/process-compose.log`
- `logs/queue_daemon.log`

## Develop

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
