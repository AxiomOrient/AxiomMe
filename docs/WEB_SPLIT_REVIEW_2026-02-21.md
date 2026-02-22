# Web Split Review (2026-02-21)

## Goal

Separate web/viewer delivery from the main runtime repository while keeping `axiomme-core` and CLI stable, deterministic, and releaseable.

Target layout:

- `/Users/axient/repository/AxiomMe` (core runtime + CLI + release gates)
- `/Users/axient/repository/AxiomMe-web` (viewer/web app only)

## Current Coupling Points

In `AxiomMe`:

- `crates/axiomme-cli/Cargo.toml` has a direct dependency on `axiomme-web`.
- `crates/axiomme-cli/src/commands/mod.rs` directly imports:
  - `render_markdown_preview`
  - `serve_web`

This means CLI build/release transitively includes web dependencies and runtime behavior.

## Recommendation

Proceed with split. It improves clarity and release control:

- CLI release can stay focused on deterministic batch/automation flows.
- Web can evolve UI/HTTP concerns independently.
- Performance troubleshooting becomes simpler (interactive server path isolated).

## Suggested Split Strategy

1. Define stable API contract first
- Freeze request/response schema for document/fs/editor endpoints.
- Keep contract in `AxiomMe` as source-of-truth markdown + JSON examples.

2. Remove direct CLI->web compile dependency
- Keep `axiomme-cli` dependent on `axiomme-core` only.
- Replace `web` command behavior with one of:
  - exec external viewer binary (`axiomme-webd`) if installed
  - or print explicit handoff message with host/port/run command

3. Move viewer/web crate into new repo
- New repo: `/Users/axient/repository/AxiomMe-web`
- Pin `axiomme-core` via git tag/revision (not floating `main`).

4. Add compatibility gate
- In CI, run contract tests that validate viewer responses against frozen API fixtures.
- Fail fast on contract drift.

## Incremental Rollout (Low Risk)

Phase 1 (now):

- Keep existing behavior.
- Introduce an adapter boundary in CLI for web calls (single module).

Phase 2:

- Publish/prepare `AxiomMe-web`.
- Switch CLI `web` command to external process handoff.

Phase 3:

- Remove `axiomme-web` dependency from `AxiomMe` workspace.
- Keep only shared contracts and compatibility tests.

## Risks and Controls

- Risk: contract drift between runtime and viewer.
  - Control: versioned contract + fixture tests in both repos.
- Risk: operator confusion during transition.
  - Control: explicit CLI error/help text when viewer binary is missing.
- Risk: release coupling persists accidentally.
  - Control: enforce `axiomme-cli` dependency graph check (no `axiomme-web`).

## Decision

`GO` for split, but only after API contract freeze and dependency boundary introduction.
