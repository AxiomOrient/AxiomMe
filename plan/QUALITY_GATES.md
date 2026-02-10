# Quality Gates

All gates are blocking. If one fails, promotion is denied.

## G0: Documentation Integrity

Checks:

- `docs/FEATURE_SPEC.md` and `docs/API_CONTRACT.md` are mutually consistent.
- `plan/IMPLEMENTATION_PLAN.md`, `plan/TASKS.md`, `plan/QUALITY_GATES.md` are mutually consistent.
- Canonical protocol in all docs is `axiom://`.

Threshold:

- 0 contradictions on API signatures, protocol, scope names.

## G1: Build and Static Quality

Checks:

- `cargo check --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`

Threshold:

- 0 failures, 0 warnings.

## G2: Naming and Protocol Purge

Checks:

- Prohibited legacy tokens are absent from source/docs/tests/runtime templates.
- URI parser, filesystem, state payloads, CLI examples use `axiom://`.

Threshold:

- 0 prohibited-token matches.
- 100% URI roundtrip tests pass with `axiom://`.

## G3: Behavioral Equivalence for Replacements

Checks:

- Each previously marked replacement path has explicit equivalence tests:
  - ingest-finalize pipeline equivalence,
  - tier generation equivalence,
  - replay/recovery equivalence.
- Failure-mode parity and observability parity are verified.

Threshold:

- 0 unresolved equivalence gaps.

## G4: Embedding Reliability

Checks:

- Pluggable embedding interface is implemented.
- At least one semantic embedding backend is production-ready in local profile.
- Deterministic fallback remains available and tested.
- Retrieval quality regression suite is active.
- Benchmark gate enforces nDCG@10/Recall@10 regression <= 3% against previous eligible benchmark.

Threshold:

- nDCG@10 >= 0.75
- Recall@10 >= 0.85
- quality regression <= 3% against previous eligible benchmark

## G5: Retrieval Correctness and Safety

Checks:

- `find/search` filters and limit behavior are deterministic across backends.
- trace completeness is validated.
- package import blocks traversal and unsafe extraction.

Threshold:

- trace completeness >= 99.9%
- filter mismatch incidents = 0

## G6: Operability and Recovery

Checks:

- request logs include request id and trace id.
- replay/reconcile restores consistency after restart.
- release evidence artifacts are generated.

Threshold:

- post-reconcile drift count = 0
- replay recovery success = 100%

## G7: Release Readiness

Checks:

- G0..G6 all pass.
- blocker count is zero.

Threshold:

- unresolved blockers = 0
