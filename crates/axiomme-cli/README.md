# axiomme-cli

Command-line interface crate.

## Responsibility

- Parse commands/options and map them to `axiomme-core` service calls.
- Expose web server boot command via `axiomme-web`.
- Keep CLI behavior deterministic/script-friendly.
- Provide filesystem/document parity commands for web workflows:
  - filesystem: `mkdir`, `rm`, `mv`, `tree`
  - document: `document load|preview|save`

## Runtime Policy

- Runtime preparation is intentionally selective:
  - `init` runs bootstrap only (filesystem/state/backend boot).
  - Retrieval-heavy commands run runtime prepare (`abstract`, `overview`, `find`, `search`, `trace replay`, `eval run`, `benchmark run|amortized`, `release pack`, `web`).
  - Read-only/ops commands avoid full runtime prepare (`ls`, `glob`, `read`, `tree`, `queue status/replay/work/daemon/evidence`, `trace list/get/stats/...`, `benchmark list/trend/gate`, `session list/delete`, etc.).
- Goal: avoid unnecessary global tier/index rebuild on commands that do not need retrieval runtime state.

## How To Run (Operator)

```bash
cargo run -p axiomme-cli -- --help
cargo run -p axiomme-cli -- benchmark amortized --iterations 5 --query-limit 120 --search-limit 10
```

Benchmark notes:
- `benchmark run` defaults to `--include-stress=true` and `--trace-expectations=false`.
- Enable trace-derived labels explicitly when needed: `--trace-expectations`.
- `benchmark gate` supports optional stress floor: `--min-stress-top1-accuracy <float>`.
- `release pack` forwards optional stress floor to benchmark gate:
  `--benchmark-min-stress-top1-accuracy <float>`.
- `release pack` `G0` is executable contract integrity (contract probe test pass), not markdown/workflow existence checks.
- Security audit modes:
  - `security audit --mode offline|strict` (`offline` default)
  - `release pack --security-audit-mode offline|strict` (`strict` default for release-grade checks)
  - Note: `G5` passes only with `--security-audit-mode strict`.
  - Advisory DB path policy:
    - `AXIOMME_ADVISORY_DB` set: use that exact path.
    - else default: `<workspace>/.axiomme/advisory-db`.
  - Strict mode auto-recovers invalid non-git advisory DB directory and bootstraps fresh advisory data.
  - Offline mode does not fetch; run strict once first to bootstrap advisory DB.
  - Strict mode fetches fresh advisory data; environment must allow network access and advisory DB writes.
- `release pack` runs `G6` in candidate mode by default (`gate_profile=rc-candidate`, `write_release_check=false`).
  - If you need strict release policy, run `benchmark gate` with release profile and release-check output:
    `--gate-profile rc-release --write-release-check`.

Add command target semantics:
- `add --target` is a destination root URI (directory semantics).
- For file ingestion, the source filename is preserved under the target URI.
- Example: `add ./note.md --target axiom://resources/smoke` => `axiom://resources/smoke/note.md`.

Retrieval backend:
- Select backend explicitly with:
  - `AXIOMME_RETRIEVAL_BACKEND=memory` (default)
  - `sqlite` is not supported and fails fast as configuration error

## How To Extend (Developer)

1. Add/modify command schema in [`src/cli/mod.rs`](./src/cli/mod.rs) and related files under [`src/cli/`](./src/cli/).
2. Keep command handlers thin and delegate business logic to `axiomme-core`.
3. Validate with:
   `cargo clippy -p axiomme-cli --all-targets -- -D warnings && cargo test -p axiomme-cli`
4. Sanity-check command surfaces:
   - `cargo run -p axiomme-cli -- --help`
   - `cargo run -p axiomme-cli -- benchmark gate --help`
   - `cargo run -p axiomme-cli -- release pack --help`

## Queue Command Migration Note

- Removed: `axiomme queue inspect`
  - Use: `axiomme queue status`
- Removed: `axiomme wait` (top-level alias)
  - Use: `axiomme queue wait [--timeout-secs N]`
