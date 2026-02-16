# axiomme-web

Web server/editor crate.

## Responsibility

- Serve markdown/document editor UI assets.
- Expose HTTP APIs for document/markdown save/load and filesystem operations.
- Apply security headers and markdown sanitization.
- Run startup reconcile/recovery before serving traffic.

## How To Run (Operator)

```bash
cargo run -p axiomme-cli -- web --host 127.0.0.1 --port 8080
```

## How To Extend (Developer)

1. Add routes/wiring in [`src/lib.rs`](./src/lib.rs).
2. Implement API behavior in [`src/handlers.rs`](./src/handlers.rs) and DTOs in [`src/dto.rs`](./src/dto.rs).
3. Keep security invariants in [`src/security.rs`](./src/security.rs) and markdown sanitizer in [`src/markdown.rs`](./src/markdown.rs).
4. Validate with:
   `cargo clippy -p axiomme-web --all-targets -- -D warnings && cargo test -p axiomme-web`
