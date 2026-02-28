# Ontology Schema Evolution Policy

Scope: `axiomme-core` ontology contract (`axiom://agent/ontology/schema.v1.json`)

## 1. Objective

Keep ontology evolution explicit, versioned, and mechanically safe.

Constraints:

- no hidden compatibility behavior
- no implicit field-level fallback across major schema versions
- deterministic parse/compile outcomes

Current project state:

- AxiomMe ontology contract is not publicly deployed yet.
- Until first external release, schema evolution may use direct cutover without legacy migration support.

## 2. Current Contract (v1)

- Schema version: `1`
- Canonical schema URI: `axiom://agent/ontology/schema.v1.json`
- Parser behavior: strict (`deny_unknown_fields`)
- Validation behavior: pure and side-effect free

Because parsing is strict, adding unknown fields to `v1` is a breaking change and must not be done.

## 3. Pre-release Evolution Mode (Current Rule)

Because this project is not externally released yet, ontology evolution uses one explicit rule:

1. keep a single active schema major at runtime
2. allow direct cutover (`v1` -> `v2`) in one change set
3. do not maintain dual parser/validator paths before release
4. keep strict parser behavior (`deny_unknown_fields`) to avoid hidden fallback

## 4. Direct Cutover Procedure (`v1` -> `v2`)

1. Define `OntologySchemaV2` contract first (types, docs, tests).
2. Switch canonical artifact (`schema.v2.json`) and parser/validator wiring in the same PR.
3. Run release gates and ontology pressure trend checks.
4. Remove stale `v1` write path logic in the same PR to avoid dead branches.

## 5. Release Gate Policy

`G0` contract integrity must verify:

- ontology probe test execution succeeds
- canonical default ontology schema parses and compiles
- schema version matches required policy version

## 6. V2 Escalation Trigger Rule

Use ontology pressure trend gate as data contract:

- source snapshots:
  - `logs/ontology_pressure_snapshot_ci.json`
  - `logs/ontology_pressure_snapshot_nightly.json`
- trend policy:
  - `min_samples = 3`
  - `consecutive_v2_candidate = 3`
- trigger condition:
  - `status == trigger_v2_design`
  - equivalently, last 3 samples are consecutive `v2_candidate=true` and total samples >= 3

This rule is evaluated by:

- `axiomme ontology trend`
- `scripts/ontology_pressure_trend_gate.sh`
- CI/nightly workflows

## 7. Versioning Semantics (Pre-release)

- Direct schema major cutover (`v1` -> `v2`): allowed.
- Contract-preserving runtime/perf change: patch release.
- If external release starts later, add a separate public compatibility policy document.
