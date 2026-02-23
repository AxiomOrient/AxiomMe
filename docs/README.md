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

## Rule

- The canonical URI scheme is `axiom://`.
- Naming, protocol, examples, and acceptance text must follow the same source of truth.
- AxiomMe runtime remains standalone at process level, and OM logic is integrated through `axiomme-core::om` with `episodic` as the default pure OM engine dependency.
- Web viewer delivery is externalized; this repository owns core/CLI/FFI contracts only.
- Local/CI quality verification entrypoint is `bash scripts/quality_gates.sh`.
- Strict release-gate CI probe entrypoint is `bash scripts/release_pack_strict_gate.sh`.
- Typed-edge enrichment latency delta probe entrypoint is `bash scripts/typed_edge_enrichment_probe.sh`.
- Ontology pressure snapshot probe entrypoint is `bash scripts/ontology_pressure_snapshot.sh`.
- Ontology pressure trend gate probe entrypoint is `bash scripts/ontology_pressure_trend_gate.sh`.
- Retrieval backend is memory-only; `AXIOMME_RETRIEVAL_BACKEND` accepts `memory` only.
