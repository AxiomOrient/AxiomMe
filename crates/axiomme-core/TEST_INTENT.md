# axiomme-core Test Intent (Pseudo Code)

이 문서는 `axiomme-core` 핵심 기능의 테스트 의도를 pseudo code 수준으로 명시한다.
목표는 테스트 이름만으로 부족한 "무엇을 왜 검증하는지"를 유지보수 가능한 형태로 고정하는 것이다.

## 1) Runtime Lifecycle

Pseudo:

```text
Given empty root
When bootstrap()
Then scope directories exist
And runtime index artifacts are not required yet

Given initialized runtime
When prepare_runtime()/initialize()
Then scope tiers and runtime index are ready
```

Primary tests:

- `crates/axiomme-core/src/client/tests/initialization_lifecycle.rs`

## 2) Filesystem Safety Boundary

Pseudo:

```text
Given symlink that points outside root
When read/write/read_tiers/write_tiers
Then operation is rejected with SecurityViolation

Given non-system write to queue scope
When write/append/mkdir
Then operation is denied
```

Primary tests:

- `crates/axiomme-core/src/fs.rs`

## 3) Ingest -> Reindex -> Retrieval Pipeline

Pseudo:

```text
Given source files
When add_resource(wait=true)
Then files are ingested and indexed
And find/search returns matching hits

Given large file
When reindex
Then indexed text is truncated deterministically and marked as truncated
```

Primary tests:

- `crates/axiomme-core/src/client/tests/core_editor_retrieval.rs`
- `crates/axiomme-core/src/client/indexing_service.rs`

## 4) Symlink Isolation During Indexing/Packaging

Pseudo:

```text
Given symlink entry under resources
When reindex_uri_tree
Then symlink entry is skipped
And external target content is not searchable

Given ovpack export source tree with symlink entry
When export_ovpack
Then symlink entry is excluded from archive

Given ovpack export source root is symlink
When export_ovpack
Then operation fails with SecurityViolation
```

Primary tests:

- `crates/axiomme-core/src/client/indexing_service.rs`
- `crates/axiomme-core/src/pack.rs`
- `crates/axiomme-core/src/client/release/benchmark_service.rs`

## 5) Queue Replay/Reconcile Reliability

Pseudo:

```text
Given queued events
When replay_outbox
Then status moves new->processing->done (or requeued/dead_letter by policy)
And checkpoints advance deterministically

Given index/fs drift
When reconcile_state
Then stale entries are pruned
And selected scopes are reindexed
```

Primary tests:

- `crates/axiomme-core/src/client/tests/queue_reconcile_lifecycle.rs`
- `crates/axiomme-core/src/state/queue.rs`

## 6) Editor Consistency (ETag / Rollback)

Pseudo:

```text
Given document with etag
When save with stale etag
Then conflict is returned

Given save succeeds but reindex fails
When save_markdown/save_document
Then file content rolls back to previous state
```

Primary tests:

- `crates/axiomme-core/src/client/tests/core_editor_retrieval.rs`

## 7) Session Data Lifecycle

Pseudo:

```text
Given session content indexed in session scope
When delete(session_id)
Then session tree is removed
And session-prefixed search/index_state entries are pruned
And deleting again returns false
```

Primary tests:

- `crates/axiomme-core/src/client/tests/core_editor_retrieval.rs`

## 8) Release Evidence / Gates

Pseudo:

```text
Given release evidence commands
When collect_operability_evidence / collect_reliability_evidence / run_security_audit
Then report artifact is persisted and contract fields are populated

Given invalid workspace path or missing Cargo.toml
When collect_release_gate_pack
Then fail fast before expensive gate execution

Given minimal workspace fixture and mocked cargo workspace commands
When collect_release_gate_pack
Then gate decisions G0..G8 are emitted
And G0/G1 pass without invoking host workspace toolchain state

Given workspace missing axiomme-core contract probe target
When evaluate_contract_integrity_gate
Then G0 fails with missing core contract probe evidence

Given security audit mode strict
When advisory refresh/execution fails
Then dependency_vulnerabilities check fails for G5
```

Primary tests:

- `crates/axiomme-core/src/client/tests/release_contract_pack_tracemetrics.rs`
- `crates/axiomme-core/src/release_gate.rs`

## 9) Embedding Strict/Fallback Behavior

Pseudo:

```text
Given strict semantic-model-http embedder
When request fails or response is invalid
Then strict error is recorded once
And fallback embedding still returns fixed-dim vector
```

Primary tests:

- `crates/axiomme-core/src/embedding.rs`

## 10) Resource Navigation API Contract

Pseudo:

```text
Given client mkdir API
When target scope is internal(queue/temp)
Then mkdir must fail with permission denied

Given client mkdir API on resources scope
When mkdir succeeds
Then directory exists and reindex event is queued

Given files under a resources subtree
When tree/glob APIs are called
Then filesystem view is reflected at client API boundary
```

Primary tests:

- `crates/axiomme-core/src/client/tests/core_editor_retrieval.rs`
