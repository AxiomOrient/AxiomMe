# Implementation Plan

Date: 2026-02-10

## 1. Planning Stance

This plan follows three operating principles:

- Product minimalism: reduce surface area and remove ambiguity first.
- Clean boundaries: isolate policy from implementation details.
- Data-centric iteration: ship measurable changes, then tune with evidence.

## 2. Mandatory Outcomes

1. Canonical protocol is `axiom://` across parser, storage, CLI, tests, and docs.
2. Prohibited legacy terms are fully removed from repository text and runtime-visible strings.
3. Previously reclassified replacement paths are proven equivalent by tests and runtime evidence.
4. Embedding layer is upgraded from brittle lexical hashing to a pluggable architecture with a semantic backend.
5. Known retrieval correctness defects are fixed (filter parity, result-limit behavior, budget exposure).

## 3. Scope Policy

- Legacy compatibility is not required.
- Legacy artifacts can be removed once replacement behavior is verified.
- Backward compatibility is optional; correctness and coherence are priority.

## 4. Risk Review

### R1: Global protocol/naming migration breaks hidden string contracts

Mitigation:

- Phase protocol swap in one bounded change-set.
- Add repository-wide prohibited-token scan before merge.
- Keep URI parse/format and persisted payload schema conformance tests.

### R2: Replacement paths are similar but not equivalent under failures

Mitigation:

- Define equivalence matrix with three dimensions:
  - behavior equivalence,
  - failure-mode equivalence,
  - observability equivalence.
- Gate release on matrix completeness.

### R3: Current embedding path degrades retrieval quality

Mitigation:

- Introduce `Embedder` trait and backend strategy.
- Integrate at least one semantic local backend.
- Maintain deterministic fallback with explicit lower-tier profile.
- Track nDCG/Recall regressions in CI.

### R4: Partial bug fixes can leave backend inconsistency

Mitigation:

- Fix retrieval defects together and verify cross-backend parity in one suite.
- Add dedicated tests for MIME filtering and limit truncation in every retrieval mode.

## 4.1 Replacement Reclassification Audit

The following areas were previously treated as replacement-complete and were re-audited with stricter criteria.

| Area | Current Implementation | Equivalence Verdict | Disposition | Owner | Target Date | Tracking |
|---|---|---|---|---|---|---|
| Temp ingest + finalize pipeline | Integrated ingest/session/replay path with explicit equivalence tests | Equivalent | Closed | Runtime/Core | 2026-02-10 | AX-P2-02 (DONE) |
| Tier synthesis (L0/L1) | Deterministic template generation plus optional semantic synthesis mode (`AXIOMME_TIER_SYNTHESIS=semantic-lite`) | Equivalent for current phase policy | Closed | Search/Core | 2026-02-10 | AX-P3-06 (DONE) |
| Retrieval ranking without dedicated reranker | Hybrid rank-fusion (RRF) plus document-type-aware reranker (`doc-aware-v1`) | Equivalent for current phase policy | Closed | Retrieval/Core | 2026-02-10 | AX-P4-04 (DONE) |
| Hash-based embedding | `semantic-lite` default with deterministic hash fallback policy | Equivalent for current phase policy | Closed | Runtime/Core | 2026-02-10 | AX-P3-02, AX-P3-03, AX-P3-05 (DONE) |

## 5. Phased Execution

### Phase P0 (Completed): Documentation Normalization

- Removed obsolete/duplicate docs.
- Reduced active docs to canonical set.
- Updated protocol and naming constraints in docs.

### Phase P1: Protocol + Naming Migration

- Replace URI scheme implementation with `axiom://`.
- Migrate parser/formatter, FS mapping, state payload strings, CLI output.
- Remove prohibited legacy tokens from source/docs/tests/log templates.
- Enforce scan in CI.

Deliverable:

- All repository-visible protocol strings are canonical.

### Phase P2: Replacement-Equivalence Validation

- Audit all previously labeled replacement implementations.
- Add equivalence tests for ingest/finalize, tier generation, replay/recovery.
- Add observability checks to prove operational parity.

Deliverable:

- Replacement-equivalence report with zero unresolved gaps.

### Phase P3: Embedding Architecture Redesign

- Introduce `Embedder` trait.
- Implement two providers:
  - semantic backend for production profile,
  - deterministic fallback backend for constrained profile.
- Add vector-version stamping and forced reindex policy on stamp mismatch.
- Add semantic tier synthesis backend option while retaining deterministic fallback path.

Deliverable:

- Stable embedding abstraction and measurable retrieval quality improvement.

### Phase P4: Retrieval Correctness Hardening

- Fix backend MIME filter parity.
- Fix hybrid/backend result-limit truncation behavior.
- Expose search budget controls in API and CLI.
- Add optional reranker extension point and benchmark-driven validation loop.

Deliverable:

- Deterministic and backend-consistent retrieval behavior.

### Phase P5: Legacy Pruning and Lockdown

- Remove dead code and stale artifacts.
- Rewrite tests to canonical naming/protocol only.
- Run full gates and freeze baseline.

Deliverable:

- Clean repository with zero legacy naming/protocol residue.

## 6. Embedding Decision Framework

## Problem

Current lexical hashing is deterministic and cheap but can fail on semantic similarity, multilingual mapping, synonym handling, and long-context retrieval.

## Decision

Adopt a layered embedding policy:

- Tier 1 (default release): semantic local model backend.
- Tier 2 (fallback): deterministic hashing backend.

## Requirements for semantic backend

- Local/offline capable.
- Stable dimension and reproducible model versioning.
- Acceptable latency for single-node runtime.

## Quality gates

- nDCG@10 >= 0.75
- Recall@10 >= 0.85
- Regression <= 3% vs previous eligible benchmark

## 7. Success Criteria

The plan is complete only when all are true:

1. `axiom://` is the only URI protocol in repository/runtime-facing text.
2. Prohibited legacy terms count is zero.
3. Replacement-equivalence matrix is fully green.
4. Embedding redesign passes retrieval quality gates.
5. Retrieval correctness defects are fixed and covered by regression tests.
6. Full build/test/lint/format gates pass.
