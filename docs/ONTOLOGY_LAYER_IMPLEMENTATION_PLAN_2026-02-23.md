# Ontology Layer Implementation Plan

Date: 2026-02-23  
Scope: `/Users/axient/repository/AxiomMe` (`axiomme-core`, `axiomme-cli`, `axiomme-mobile-ffi`)

## Objective

Define one explicit ontology layer that keeps AxiomMe simple, data-first, and operationally predictable.

Target outcome:

1. Domain meaning is modeled as explicit data contracts.
2. Transformations stay pure and testable.
3. Side effects remain isolated to existing runtime boundaries.
4. Topology (how entities connect and flow) becomes queryable and enforceable.

## Self Feedback Loop (Applied)

Iteration 1 (error): considered direct RDF/OWL-first adoption as primary path.  
Correction: this adds accidental complexity early and weakens control over runtime cost.

Iteration 2 (error): focused only on relation typing, not action semantics.  
Correction: ontology must include both noun model (`object/link`) and verb model (`action/invariant`).

Iteration 3 (error): proposed large storage redesign upfront.  
Correction: keep rollout direct and explicit; enforce contracts on write first, then optimize read path.

Final decision after corrections: add a native, explicit ontology contract layer inside `axiomme-core`, backed by versioned JSON schema and pure validators.

## Current Baseline (Code Reality)

1. URI and scope model is explicit and stable (`axiom://{scope}/{path}`) in `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/uri.rs`.
2. Relation storage exists (`.relations.json`) with structural validation only in `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/fs.rs`.
3. Relation APIs exist in `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/client/relation_service.rs`.
4. Retrieval is weighted search, not ontology reasoning, in `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/index.rs`.
5. Queue topology is event-lane routing (`semantic`, `embedding`) in `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/state/queue_lane.rs`.

Gap: there is no explicit ontology schema that defines entity types, allowed links, action semantics, or invariants.

## Options Considered

## Option A: Keep current relation model only

Pros:

1. No implementation cost.
2. No schema transition work.

Cons:

1. No typed semantics.
2. Weak guardrails for agent behavior.
3. Limited ontology/topology growth.

## Option B: Full RDF/OWL stack as core runtime

Pros:

1. Strong formal semantics.
2. Existing ecosystem tooling.

Cons:

1. High complexity and integration overhead.
2. Harder performance predictability on mobile/offline.
3. Conflicts with current simple runtime philosophy.

## Option C (Selected): Native Contract-First Ontology Layer

Pros:

1. Fits existing data model and boundary style.
2. Maintains explicit control of allocation/latency.
3. Controlled rollout without hidden adapters.

Cons:

1. Requires disciplined schema governance.
2. Must build some tooling ourselves.

Selected because it maximizes clarity and mechanical sympathy with current architecture.

## Chosen Architecture

Add a new internal layer in `axiomme-core`:

`ontology (pure contracts + pure validation)` -> `relation_service/search/queue (effects)`

### 1) Data Contracts (Explicit, Versioned)

New schema artifact:

- `axiom://agent/ontology/schema.v1.json` (runtime source of truth)

New Rust module:

- `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/ontology/`

Core types (v1):

1. `OntologySchemaV1`
2. `ObjectTypeDef` (`id`, `uri_prefixes`, `required_tags`, `allowed_scopes`)
3. `LinkTypeDef` (`id`, `from_types`, `to_types`, `min_arity`, `max_arity`, `symmetric`)
4. `ActionTypeDef` (`id`, `input_contract`, `effects`, `queue_event_type`)
5. `InvariantDef` (`id`, `rule`, `severity`, `message`)

### 2) Pure Validation Pipeline

Pure functions only:

1. Parse and validate schema.
2. Resolve URI to object type.
3. Validate link write (`link_id`, endpoints, arity, scope policy).
4. Validate action request against action definition.
5. Evaluate invariants against in-memory projections.

No filesystem/sqlite/network side effects in this module.

### 3) Side Effect Integration Points

1. `relation_service.link/unlink`:
   call ontology validator before persisting `.relations.json`.
2. queue event creation:
   require declared `ActionTypeDef` for ontology actions.
3. retrieval:
   optional typed-query expansion uses `LinkTypeDef` metadata, bounded by budget.

### 4) Topology Model

Topology in v1 is explicit as:

1. typed nodes (`ObjectTypeDef`)
2. typed edges (`LinkTypeDef`)
3. typed transitions (`ActionTypeDef`)

This gives an ontology topology graph without importing a heavy semantic web runtime.

## Performance and Ownership Policy

1. Schema parse: once per process start, immutable cache (`Arc`).
2. Link validation: O(k) over endpoints + O(1) type lookup via hash maps.
3. Invariant checks:
   background/explicit command path only, not on every read.
4. Memory budget:
   keep one compiled schema instance; no per-request cloning.
5. Mobile safety:
   no host subprocess dependence in ontology path.

## Implementation Plan (Phased)

## Phase 0: Contract Scaffolding

1. Add `ontology` module and v1 data structs.
2. Add schema parser + validator tests.
3. Add sample schema file under `agent/ontology` bootstrap path.

Exit criteria:

1. no behavior change to existing commands when schema is absent.
2. `cargo test -p axiomme-core` green.

## Phase 1: Write Path Enforcement

1. Integrate validation in `relation_service.link`.
2. Enforce `link_id` must map to declared `LinkTypeDef`.
3. Return deterministic validation errors with explicit codes.

Exit criteria:

1. invalid typed links are rejected.
2. relation flows that satisfy declared contracts remain valid.

## Phase 2: Read Path Topology Use

1. Add typed relation metadata in find/search enrichment.
2. Add optional topology traversal helper with explicit budget controls.

Exit criteria:

1. retrieval output can explain typed edges.
2. latency regression stays within existing budget targets.

## Phase 3: Action and Invariant Layer

1. Define ontology action command contract.
2. Validate action inputs before enqueue.
3. Add invariant check command and report output.

Exit criteria:

1. actions are schema-governed.
2. invariant report is reproducible and machine-readable.

## Phase 4: Release Gate Integration

1. Add ontology contract probe in release gate (`new gate item`).
2. Add schema version-policy checks.
3. Fail release when ontology contract violations exist above threshold.

Exit criteria:

1. release artifacts include ontology contract evidence.

## Task Backlog (Executable)

1. `ONT-001` Add module skeleton:
   `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/ontology/{mod.rs,model.rs,parse.rs,validate.rs}`
2. `ONT-002` Define `OntologySchemaV1` serde contract + version field.
3. `ONT-003` Implement schema sanity checks (duplicate ids, invalid scopes, arity errors).
4. `ONT-004` Add URI-to-object-type resolver using prefix rules.
5. `ONT-005` Wire link validation into `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/client/relation_service.rs`.
6. `ONT-006` Add error taxonomy for ontology violations in `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/error.rs`.
7. `ONT-007` Add CLI command group `axiomme ontology validate` in `axiomme-cli`.
8. `ONT-008` Add deterministic fixture schema and relation tests.
9. `ONT-009` Add retrieval typed-edge enrichment (behind config toggle).
10. `ONT-010` Add release gate ontology contract probe and report fields.
11. `ONT-011` Update docs (`FEATURE_SPEC.md`, `API_CONTRACT.md`) with ontology contract section.
12. `ONT-012` Add explicit schema evolution policy (`v1 -> v2`).
13. `ONT-013` Add explicit `v2` pressure report contract + CLI command (`axiomme ontology pressure`).

## Implementation Status (Current)

Completed:

1. `ONT-001` module skeleton added (`ontology/{mod.rs,model.rs,parse.rs,validate.rs}`).
2. `ONT-002` `OntologySchemaV1` and related contracts added.
3. `ONT-003` schema sanity checks implemented in compile step.
4. `ONT-004` URI-prefix object type resolver implemented (longest-prefix match).
5. `ONT-005` relation write path now validates ontology link contracts when schema exists.
6. `ONT-006` ontology-specific error taxonomy added (`ONTOLOGY_VIOLATION`).
7. `ONT-007` CLI command added: `axiomme ontology validate`.
8. `ONT-008` deterministic unit/integration tests added for parse/compile/validation and relation write enforcement.
9. `ONT-009` retrieval typed-edge enrichment implemented behind config toggle (`AXIOMME_SEARCH_TYPED_EDGE_ENRICHMENT`), with typed relation metadata (`relation_type`, `source_object_type`, `target_object_type`) and query-plan visibility notes.
10. `ONT-010` release gate ontology contract probe integrated into `G0` with explicit policy/probe report fields.
11. `ONT-011` ontology contract sections added to canonical docs (`FEATURE_SPEC.md`, `API_CONTRACT.md`).
12. `ONT-012` schema evolution policy documented (`ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md`).
13. `ONT-013` ontology `v2` pressure evaluator added as pure transform with explicit policy/report contract, exposed via CLI command `axiomme ontology pressure`.
14. Phase 3 action/invariant layer implemented:
    - `axiomme ontology action-validate` / `action-enqueue` contracts
    - pure action request validation (`action_id`, `queue_event_type`, `input_contract`)
    - pure invariant evaluation report + enforceable CLI (`axiomme ontology invariant-check --enforce`)

Pending:

1. none for v1 baseline plan

## Task Order (Critical Path)

1. `ONT-001 -> ONT-004 -> ONT-005 -> ONT-008`
2. `ONT-006 -> ONT-007`
3. `ONT-009`
4. `ONT-010 -> ONT-011 -> ONT-012`

## Acceptance Criteria

1. Ontology schema is explicit, versioned, and validated.
2. Invalid links/actions fail before persistence/enqueue.
3. Existing core URI/scope invariants remain unchanged.
4. No new hidden side effects are added to pure modules.
5. Build/test baseline remains green:
   - `cargo check --workspace`
   - `cargo test -p axiomme-core`
   - `cargo test -p axiomme-cli`

## Non-Goals (v1)

1. No mandatory RDF/OWL/SPARQL runtime integration.
2. No distributed graph database introduction.
3. No automatic semantic inference engine beyond declared rules.

## Recommended Next Focus

1. run release-gate pack end-to-end with strict security mode in CI
2. collect typed-edge enrichment latency deltas with and without toggle
3. collect `ontology pressure` reports over real schemas and design `OntologySchemaV2` only when pressure threshold is repeatedly crossed

## Next Focus Status (2026-02-23)

Completed:

1. CI strict release-pack execution path added via `scripts/release_pack_strict_gate.sh` and `.github/workflows/quality-gates.yml`.
2. Typed-edge enrichment delta probe added via `scripts/typed_edge_enrichment_probe.sh` and wired into nightly perf workflow.
3. Relation enrichment path hardened with per-request owner cache to avoid repeated `.relations.json` reads across `query_results` in a single find/search request (category views are derived via `hit_buckets` indices).
4. `axiomme ontology pressure` command added with explicit `OntologyV2PressurePolicy` / `OntologyV2PressureReport` contract for data-driven `v2` escalation.
5. Ontology pressure snapshots are now persisted in both CI and nightly artifacts via `scripts/ontology_pressure_snapshot.sh`, wired into `.github/workflows/quality-gates.yml` and `.github/workflows/perf-regression-nightly.yml`.
6. Automated trend rule implemented (`min_samples=3`, `consecutive_v2_candidate=3`) with `axiomme ontology trend` + `scripts/ontology_pressure_trend_gate.sh`, wired in CI/nightly.
7. Trend policy contract hardened with explicit positive constraints (`min_samples>=1`, `consecutive_v2_candidate>=1`) at CLI parse and core validator levels.
8. Ontology action/invariant contracts are now explicit and executable:
   - action request validation is pure and reusable in core (`validate_action_request`)
   - invariant evaluation is pure and machine-readable (`evaluate_invariants`)
   - enqueue side effect path validates contract first (`ontology action-enqueue`)
9. Release gate ontology probe now carries invariant-evaluation outcomes (`invariant_check_passed`, `invariant_check_failed`) and fails `G0` when invariant failures exist.
10. Action input contract semantics hardened to explicit allow-list:
    - supported contracts: `json-any|json-null|json-boolean|json-number|json-string|json-array|json-object`
    - unknown contracts are rejected at schema compile time (no implicit fallback)

Pending:

1. none for ontology v1/v1.5 execution plan
