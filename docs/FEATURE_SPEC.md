# Feature Specification

## 1. Objective

Build a Rust-native context system with a stable local-first workflow, deterministic behavior, and measurable quality.

## 2. Hard Constraints

- Canonical URI scheme: `axiom://{scope}/{path}`
- Core scopes: `resources`, `user`, `agent`, `session`
- Internal scopes: `temp`, `queue`
- `queue` is read-only for non-system operations
- Legacy naming and legacy URI protocol tokens are prohibited in docs, code, logs, and tests

## 3. Core User Stories

1. As a developer, I can add local/remote resources and browse them like a filesystem.
2. As an agent runtime, I can retrieve context with explainable traces.
3. As a user, I can commit a session and keep useful long-term memory.
4. As an operator, I can replay/reconcile state after failures.
5. As an integrator, I can import/export packaged context trees safely.

## 4. Functional Requirements

### FR-001 URI and Scope

- Parse and normalize `axiom://{scope}/{path}`.
- Reject invalid scope and traversal patterns.
- Enforce scope-level write restrictions.

### FR-002 Tiered Context

- Every navigable directory supports:
  - L0: `.abstract.md`
  - L1: `.overview.md`
  - L2: original files
- Tier synthesis defaults to deterministic output and supports optional semantic mode via `AXIOMME_TIER_SYNTHESIS=semantic-lite`.

### FR-003 Resource Ingest

- Support local files/directories and URL inputs.
- Ingest uses temp staging and finalize move.
- Indexing/semantic updates are replay-safe and asynchronous.
- Markdown editor save path uses full-document replace with etag conflict guard and synchronous reindex.

### FR-004 Retrieval

- `find(query, target_uri?, limit?, score_threshold?, filter?)`
- `search(query, target_uri?, session?, limit?, score_threshold?, filter?)`
- Budget knobs are supported per request: `max_ms`, `max_nodes`, `max_depth`.
- Every retrieval returns ranked hits and trace metadata.
- Hybrid backend merge uses rank-based fusion to avoid score-scale mismatch.
- Post-retrieval reranking supports document-type-aware profile (`AXIOMME_RERANKER=doc-aware-v1|off`).

### FR-005 Session and Memory

- Expose session create/load, message append, usage updates.
- `commit` archives active messages and extracts memory categories.
- Updated memory is searchable after indexing.

### FR-006 Package Interop

- Export/import package format with force/vectorize controls.
- Import must block path traversal and unsafe extraction.

### FR-007 Observability and Evidence

- Persist request logs and retrieval traces.
- Generate operability, reliability, security, and release evidence artifacts.

### FR-008 Naming Migration

- All protocol strings, examples, and surface text use `axiom://`.
- Prohibited legacy terms must be removed from repository text and runtime outputs.

### FR-009 Replacement Validation

- Any previously labeled "alternative complete" area must pass explicit equivalence criteria:
  - behavior equivalence,
  - failure-mode equivalence,
  - observability equivalence.

### FR-010 Embedding Reliability

- Embedding layer must support pluggable providers.
- Provider selection must be explicit (`AXIOMME_EMBEDDER`) and local/offline only.
- Deterministic fallback is allowed, but production profile requires a semantic model backend.
- Retrieval quality gates must detect embedding regressions early.

### FR-011 Markdown Web Viewer/Edit

- Provide local web UI for markdown load/edit/save and preview.
- Save policy is full-document replace only (no partial patch).
- Save path enforces `etag` conflict checks and synchronous reindex.
- During save+reindex, markdown load/save API returns explicit lock status (`423`) instead of racing.
- Web server startup runs reconciliation gate before serving markdown endpoints.
- Markdown load/save request logs include latency/size details (`save_ms`, `reindex_ms`, `total_ms`, `content_bytes`).
- Markdown preview sanitizes raw HTML and unsafe URL schemes for links/images.
- Web responses enforce baseline security headers (CSP, no-sniff, frame deny, strict referrer, permissions policy).
- Web document endpoint supports editable load/save for `markdown`, `json`, `yaml` using full-replace policy.
- Web document viewer supports read-only load for `jsonl`, `xml`, and `txt`.

## 5. Non-Functional Requirements

- Reliability: replay/reconcile restores consistency after restart.
- Performance targets (single-node baseline):
  - `find` p95 <= 600ms
  - `search` p95 <= 1200ms
  - `session.commit` p95 <= 1500ms
- Security: traversal/scope-escape blocked across all file/package operations.
- Maintainability: explicit module boundaries, measurable acceptance criteria.

## 6. Acceptance Scenarios

### Scenario A: Resource Lifecycle

1. Add resource.
2. Wait processing.
3. Read L0/L1 and one L2 file.
4. Run `find`.

Expected: tier files exist and results are ranked with valid URIs.

### Scenario B: Traceable Retrieval

1. Query nested corpus.
2. Inspect trace.

Expected: trace includes start points, recursive steps, and stop reason.

### Scenario C: Session Memory Evolution

1. Create session.
2. Append mixed user/tool messages.
3. Commit.
4. Query memory scope.

Expected: memory files are categorized and immediately retrievable.

### Scenario D: Package Safety

1. Export tree.
2. Import to new root.
3. Retrieve imported content.

Expected: structure preserved, unsafe entries rejected.

### Scenario E: Internal Scope Governance

1. Inspect `axiom://temp` during ingest.
2. Inspect `axiom://queue` during replay.

Expected: internal scopes are visible for debugging with restrictions enforced.
