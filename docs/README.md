# Documentation Index

Canonical documentation is intentionally minimal and conflict-free.

## Canonical Files

- `README.md`
- `FEATURE_SPEC.md`
- `API_CONTRACT.md`

## Rule

- The canonical URI scheme is `axiom://`.
- Naming, protocol, examples, and acceptance text must follow the same source of truth.
- AxiomMe is a standalone runtime: OM logic is integrated in `axiomme-core` (`crate::om`) with no external `episodic`/`episodic-memory` dependency.
- Local/CI quality verification entrypoint is `bash scripts/quality_gates.sh`.
- Retrieval backend remains explicit; use `AXIOMME_RETRIEVAL_BACKEND=sqlite|memory`.
