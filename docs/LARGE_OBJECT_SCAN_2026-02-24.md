# Large Object Scan (2026-02-24)

## Command

```bash
scripts/large_object_scan.sh .
```

## Result Summary

- Scan root: `.`
- Struct field threshold: `12`
- Top files analyzed: `25`
- Top structs reported: `40`

## Top LOC Files

```text
2330  crates/axiomme-core/src/session/tests.rs
2342  crates/axiomme-core/src/index.rs
1727  crates/axiomme-core/src/client/tests/queue_reconcile_lifecycle.rs
1687  crates/axiomme-core/src/state/tests.rs
1606  crates/axiomme-core/src/session/commit.rs
1471  crates/axiomme-core/src/release_gate.rs
1258  crates/axiomme-core/src/client/tests/benchmark_suite_tests.rs
1192  crates/axiomme-core/src/client/tests/relation_trace_logs.rs
1081  crates/axiomme-core/src/session/om/tests.rs
1047  crates/axiomme-core/src/client/search/mod.rs
1004  crates/axiomme-core/src/client/search/backend_tests.rs
952   crates/axiomme-core/src/retrieval/tests.rs
948   crates/axiomme-core/src/client/tests/core_editor_retrieval.rs
914   crates/axiomme-cli/src/commands/mod.rs
```

## Top Struct Field Counts

```text
none (no structs at or above 12 fields)
```

## Review Decisions

- Runtime hot path first: optimize `index.rs` allocations/URI processing before DTO decomposition.
- `BenchmarkGateResult` decomposition completed: thresholds/quorum/snapshot/execution/artifacts are now explicit value objects.
- `BenchmarkReport` decomposition completed: selection/quality/latency/artifacts are now explicit value objects.
- `ReliabilityEvidenceReport` decomposition completed: replay plan/progress, queue delta, and search probe are now explicit value objects.
- `OperabilityEvidenceReport` decomposition completed: sample window and coverage are now explicit value objects.
- `FinalizeSingleQueryInput` decomposition completed: finalize path now uses explicit context/candidates/trace inputs.
- `OntologyContractProbeResult` decomposition completed: schema/version/cardinality/invariant summaries are now explicit nested contracts.
- `ReleaseGatePackOptions` decomposition completed: replay/operability/eval/benchmark/security plans are now explicit grouped contracts.
- `EvalReportInput` decomposition completed: report write path now uses explicit meta/run_config/coverage/outcome inputs.
- `OmObserverConfig` decomposition completed: observer runtime config now separates `llm` model controls from `text_budget`.
- `BenchmarkAmortizedReport` decomposition completed: amortized output now separates `selection`, `timing`, and `quality` summaries.
- `EvalLoopReport` decomposition completed: eval output now separates `selection`, `coverage`, `quality`, and `artifacts`.
- Current large-object scan gate status: no production structs over threshold (`12` fields).
- Test files with high LOC are accepted for now; production modules remain priority for complexity reduction.
