# Web Split Review (2026-02-21)

## Decision

`GO` for repository split. Keep runtime core and web viewer as separate delivery units.

## Final Layout (Completed)

- `/Users/axient/repository/AxiomMe`
  - `axiomme-core`
  - `axiomme-cli`
  - `axiomme-mobile-ffi`
- `/Users/axient/repository/AxiomMe-web`
  - external viewer/server runtime (`axiomme-webd`)

## Why This Is Better

- Clear release boundaries:
  - core/cli/mobile contracts are releasable without web server dependencies.
- Mobile packaging clarity:
  - iOS/Android integration uses FFI boundary only.
  - no in-process localhost web server assumptions in native app lifecycle.
- Operational clarity:
  - CLI explicitly launches external viewer process.
  - viewer failure/isolation is visible at process boundary.

## Implemented Boundary Changes

1. CLI compile dependency on web crate removed.
2. `axiomme web` changed to external process handoff.
3. `AxiomMe` workspace no longer includes `crates/axiomme-web`.
4. Viewer/server crate moved to dedicated external project.

## Contract and Drift Control

- `docs/API_CONTRACT.md` remains source of truth for web extension endpoints.
- Viewer project must validate compatibility against this contract.
- Recommended CI gates:
  - schema fixture compatibility check
  - CLI dependency graph check (no web crate linkage)

## Remaining Work (Out of Scope Here)

- Publish/version `AxiomMe-web` independently.
- Add cross-repo contract conformance tests (fixtures).
