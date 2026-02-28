# Documentation Index

Canonical documentation is intentionally minimal and conflict-free.

## Canonical Files

- `README.md`
- `FEATURE_SPEC.md`
- `API_CONTRACT.md`

## Supplemental Guides

- `ENGINEERING_PLAYBOOK_2026-02-23.md`
- `ONTOLOGY_LAYER_IMPLEMENTATION_PLAN_2026-02-23.md`
- `ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md`
- `RELEASE_NOTES_2026-02-24.md`
- `MIRROR_MIGRATION_OPERATIONS_REPORT_2026-Q2.md`

## Rule

- The canonical URI scheme is `axiom://`.
- Naming, protocol, examples, and acceptance text must follow the same source of truth.
- AxiomMe runtime remains standalone at process level, and OM logic is integrated through `axiomme-core::om` with `episodic` as the default pure OM engine dependency.
- Web viewer delivery is externalized; this repository owns core/CLI/FFI contracts only.
- Local/CI quality verification entrypoint is `bash scripts/quality_gates.sh`.
  - mirror one-cycle notice gate is executed inside quality gates and writes `logs/quality/mirror_notice_gate.json` by default.
  - mirror notice router is executed inside quality gates and writes `logs/quality/mirror_notice_router.json` by default.
  - override outputs with `AXIOMME_QUALITY_NOTICE_GATE_JSON` and `AXIOMME_QUALITY_NOTICE_ROUTER_JSON`.
  - optional strict mode: `AXIOMME_QUALITY_ENFORCE_MIRROR_NOTICE=on` fails quality gates until notice gate status becomes `ready`.
  - `.github/workflows/quality-gates.yml` enables strict mode automatically on tag pushes.
- Strict release-gate CI probe entrypoint is `bash scripts/release_pack_strict_gate.sh`.
- Mirror one-cycle notice gate entrypoint is `bash scripts/mirror_notice_gate.sh`.
- Mirror notice router entrypoint is `bash scripts/mirror_notice_router.sh`.
  - standalone default I/O paths are `logs/quality/mirror_notice_gate.json` -> `logs/quality/mirror_notice_router.json`.
  - override with `AXIOMME_QUALITY_REPORT_DIR`, `AXIOMME_QUALITY_NOTICE_GATE_JSON`, `AXIOMME_QUALITY_NOTICE_ROUTER_JSON`.
- Mirror notice router smoke entrypoint is `bash scripts/mirror_notice_router_smoke.sh`.
- Typed-edge enrichment latency delta probe entrypoint is `bash scripts/typed_edge_enrichment_probe.sh`.
- Ontology pressure snapshot probe entrypoint is `bash scripts/ontology_pressure_snapshot.sh`.
- Ontology pressure trend gate probe entrypoint is `bash scripts/ontology_pressure_trend_gate.sh`.
- Retrieval backend is memory-only; `AXIOMME_RETRIEVAL_BACKEND` accepts `memory` only.
