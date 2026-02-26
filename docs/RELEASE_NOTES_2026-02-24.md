# Release Notes (2026-02-24)

Date: 2026-02-24  
Scope: `axiomme-core`, `axiomme-cli`, docs/scripts contract updates

## Highlights

1. Retrieval fast-path now honors explicit budget constraints (`max_nodes`, `max_ms`).
2. `FindResult` now exposes canonical result structure with compatibility mirrors:
   - canonical: `query_results`, `hit_buckets`
   - compatibility mirrors: `memories`, `resources`, `skills`
3. Real-dataset matrix benchmark script now respects custom report directory paths for per-seed report outputs.

## Contract Notice: `FindResult` Mirror Field Transition

This file is the explicit release note required by the API contract for future mirror-field removal.

Applies to:

- `find`, `find_with_budget`
- `search`, `search_with_budget`, `search_with_request`

Transition policy:

1. Current cycle (2026-02-24): `memories/resources/skills` are still emitted for compatibility.
2. Migration window (2026-Q2): consumers should move to `query_results` + `hit_buckets`.
3. Removal rule: mirror fields cannot be removed before one full release cycle passes after this notice.
4. Canonical rule: when mirrors are emitted, they must always be derived from `query_results`/`hit_buckets` only.

## Migration Checklist for Consumers

1. Read primary hits from `query_results`.
2. Use `hit_buckets` index arrays for category views (`memories/resources/skills`).
3. Stop relying on mirror arrays as the source of ranking truth.
4. Treat mirror arrays as compatibility-only output during the transition window.

Operations tracking:

- `docs/MIRROR_MIGRATION_OPERATIONS_REPORT_2026-Q2.md` records migration checklist status and release-gate evidence for 2026-Q2.
- one-cycle notice completion status is tracked in section `One-cycle Notice Gate (2026-02-24 baseline)` of that report.
- latest one-cycle gate decision snapshot is persisted at `docs/MIRROR_NOTICE_GATE_2026-02-24.json`.
- latest next-action routing snapshot for that gate is persisted at `docs/MIRROR_NOTICE_ROUTER_2026-02-24.json`.

## Validation Evidence (Strict Release Pack)

Executed command:

`bash scripts/release_pack_strict_gate.sh --workspace-dir . --output docs/RELEASE_PACK_STRICT_2026-02-24.json`

Result snapshot:

1. `pack_id`: `d0d822ca-4813-405d-9183-0524fcba2e66`
2. `passed`: `true`
3. `unresolved_blockers`: `0`
4. Gate statuses: `G0..G8 = pass`
5. Report URI: `axiom://queue/release/packs/d0d822ca-4813-405d-9183-0524fcba2e66.json`
