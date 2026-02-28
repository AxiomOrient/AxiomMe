# Feature Specification

## 1. Objective

Build a Rust-native context system with a stable local-first workflow, deterministic behavior, and measurable quality.
The runtime is standalone at execution boundary, and OM is integrated with `episodic` as the default pure engine dependency.

## 2. Hard Constraints

- Canonical URI scheme: `axiom://{scope}/{path}`
- Core scopes: `resources`, `user`, `agent`, `session`
- Internal scopes: `temp`, `queue`
- `queue` is read-only for non-system operations
- Obsolete naming and obsolete URI protocol tokens are prohibited in docs, code, logs, and tests
- OM integration is explicit in `axiomme-core::om`, with `episodic` wired as the default transform/model engine.

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
- `add_resource(wait=true)` must expose explicit wait contract mode:
  - `relaxed` (default): bounded replay and return.
  - `strict`: terminal `done` 보장, timeout/dead-letter는 conflict로 반환.
- Markdown editor save path uses full-document replace with etag conflict guard and synchronous reindex.

### FR-004 Retrieval

- `find(query, target_uri?, limit?, score_threshold?, filter?)`
- `search(query, target_uri?, session?, limit?, score_threshold?, filter?)`
- Retrieval tokenization is deterministic and backend-invariant.
- Budget knobs are supported per request: `max_ms`, `max_nodes`, `max_depth`.
- Every retrieval returns ranked hits and trace metadata.
- Post-retrieval reranking is off by default; document-type-aware profile is opt-in (`AXIOMME_RERANKER=doc-aware-v1`).
- Retrieval backend is memory-only (`AXIOMME_RETRIEVAL_BACKEND=memory`).
- Invalid `AXIOMME_RETRIEVAL_BACKEND` value fails fast as runtime configuration error.

### FR-005 Session and Memory

- Expose session create/load, message append, usage updates.
- `commit` archives active messages and extracts memory categories.
- Updated memory is searchable after indexing.
- Session commit mode supports explicit `archive_only` to archive without auto extraction.
- Explicit checkpoint promotion API supports typed facts (`profile|preferences|entities|events|cases|patterns`).
- Promotion apply mode supports `all_or_nothing` and `best_effort`.

### FR-012 Boundary-Preserving Runtime vs Durable Memory

- Runtime hints are request-scoped only and must not be persisted.
- Runtime hints must not mutate session messages, OM outbox, or commit extraction inputs.
- Durable memory creation in boundary-safe flow occurs only through explicit promotion APIs.
- Promotion idempotency key is `(session_id, checkpoint_id)` and must enforce hash conflict detection.
- Promotion checkpoint state machine must use `pending -> applying -> applied`.
- Stale `applying` checkpoints must be demoted/replayed deterministically.
- Session deletion must remove corresponding promotion checkpoint rows.

### FR-006 Package Interop

- Export/import package format with force/vectorize controls.
- Import must block path traversal and unsafe extraction.

### FR-007 Observability and Evidence

- Persist request logs and retrieval traces.
- Generate operability, reliability, security, and release evidence artifacts.

### FR-008 Naming Migration

- All protocol strings, examples, and surface text use `axiom://`.
- Prohibited obsolete terms must be removed from repository text and runtime outputs.

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
- `AXIOMME_EMBEDDER` supports:
  - `semantic-lite` (default local heuristic)
  - `hash` (deterministic fallback)
  - `semantic-model-http` (local model server backend)
- `semantic-model-http` configuration:
  - `AXIOMME_EMBEDDER_MODEL_ENDPOINT` (default: `http://127.0.0.1:11434/api/embeddings`)
  - `AXIOMME_EMBEDDER_MODEL_NAME` (default: `nomic-embed-text`)
  - `AXIOMME_EMBEDDER_MODEL_TIMEOUT_MS` (default: `3000`)
  - `AXIOMME_EMBEDDER_STRICT` (`1|true|yes|on`): strict mode; model failures are recorded into benchmark run environment (`embedding_strict_error`) and release-profile gate must fail
- `semantic-model-http` endpoint must be localhost/loopback only (`127.0.0.1`, `localhost`, `::1`).
- Release-profile benchmark gates (`gate_profile` contains `release` or `write_release_check=true`) require benchmark environment embedding provider to be `semantic-model-http`.
- Benchmark gate optionally enforces a stress-query quality floor via `min_stress_top1_accuracy` (CLI: `--min-stress-top1-accuracy`).
- Release pack optionally forwards stress-query gate floor via `--benchmark-min-stress-top1-accuracy`.
- Release check artifacts must include embedding diagnostics (`embedding_provider`, `embedding_strict_error`) when release gate reasons include embedding policy failures.

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

### FR-013 Ontology Contract Layer

- Ontology source of truth is explicit data at `axiom://agent/ontology/schema.v1.json`.
- Ontology schema must be versioned (`version=1` for current contract) and validated before use.
- Relation write path must enforce declared link contracts (type/arity/endpoint coverage) when schema is present.
- Retrieval relation enrichment may include typed-edge metadata only when explicitly enabled:
  - `AXIOMME_SEARCH_TYPED_EDGE_ENRICHMENT=1|true|yes|on`
- Release gate contract integrity (`G0`) must include ontology contract probing:
  - ontology contract probe test execution,
  - schema parse/compile validation,
  - schema-version policy check,
  - invariant evaluation with zero failure count.
- CLI must expose explicit `v2` escalation pressure report (`axiomme ontology pressure`) with:
  - policy thresholds (`min_action_types`, `min_invariants`, `min_action_invariant_total`, `min_link_types_per_object_basis_points`),
  - derived counts and trigger reasons,
  - deterministic `v2_candidate` decision.
- CLI/automation must expose trend gate (`axiomme ontology trend`) with explicit policy:
  - `min_samples` (`>=1`)
  - `consecutive_v2_candidate` (`>=1`)
  - deterministic status (`insufficient_samples|monitor|trigger_v2_design`).
- CLI must expose explicit ontology action contract commands:
  - `axiomme ontology action-validate` validates `action_id`, `queue_event_type`, and `input` against schema action definitions.
  - `axiomme ontology action-enqueue` validates first, then enqueues one outbox event with explicit payload contract (`schema_version`, `action_id`, `input`).
  - action input source must be explicit and bounded (`--input-json` or `--input-file` or `--input-stdin`; at most one).
  - recognized input contracts are `json-any|json-null|json-boolean|json-number|json-string|json-array|json-object`.
  - unknown `input_contract` values are schema-compile errors.
- CLI must expose explicit invariant diagnostics command:
  - `axiomme ontology invariant-check` returns machine-readable pass/fail report per invariant (`status`, `failure_kind`, `failure_detail`).
  - `--enforce` must fail command when failed invariant count is non-zero.
  - executable invariant rule grammar subset is explicit:
    - `object_type_declared:<object_type_id>`
    - `link_type_declared:<link_type_id>`
    - `action_type_declared:<action_type_id>`
- Schema evolution policy (`v1 -> v2`) must remain explicit and documented in `docs/ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md`.

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

### Scenario D: Memory Promotion Integrity

1. Trigger `promote_session_memories` with unique `checkpoint_id`.
2. Interrupt process during I/O.
3. Retry with same `checkpoint_id` and same request hash.

Expected: 시스템은 체크포인트를 통해 중복 실행을 방지하거나, 이전 실패 지점부터 원자적으로 재개하여 데이터 무결성을 보장함.

### Scenario E: Internal Scope Governance

1. Inspect `axiom://temp` during ingest.
2. Inspect `axiom://queue` during replay.

Expected: internal scopes are visible for debugging with restrictions enforced.
