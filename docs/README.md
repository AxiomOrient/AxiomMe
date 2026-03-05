# Docs Index

## Documentation Inventory
| Path | Purpose | Status | Evidence |
|---|---|---|---|
| `README.md` | repository first-entry guide | active | root GitHub landing document |
| `docs/README.md` | canonical docs index | active | links all maintained docs |
| `docs/ARCHITECTURE.md` | architecture/layer/data-flow summary | active | runtime boundary and flow sections |
| `docs/API_CONTRACT.md` | runtime/API contract SSOT | active | client/OM/release/dependency contract sections |
| `docs/ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md` | ontology evolution policy | active | version/cutover/gate rules |

## README Link Graph
- Root README -> `crates/README.md`, `docs/README.md`
- docs/README -> `ARCHITECTURE.md`, `API_CONTRACT.md`, `ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md`
- crates/README -> crate-level READMEs (`axiomme-core`, `axiomme-cli`, `axiomme-mobile-ffi`)

## Cleanup Actions
- keep: `README.md`, `docs/README.md`, `docs/ARCHITECTURE.md`, `docs/API_CONTRACT.md`, `docs/ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md`
- delete-candidate: none (current docs set is minimal and non-duplicated)

## Rules
- API/runtime behavior는 `API_CONTRACT.md`를 단일 진실 공급원으로 유지
- 아키텍처 설명은 `ARCHITECTURE.md`에 집중
- 상세 이행 기록/실험 메모는 `docs/`에 두지 않음
