# API Contract

## 0. Policy

- Baseline behavior is defined by this document.
- Additive extensions are allowed and must be marked as `extension`.
- Canonical URI protocol is `axiom://`.
- Runtime core contracts are defined in this repository (`axiomme-core`, `axiomme-cli`, `axiomme-mobile-ffi`).
- Web viewer/server delivery is intentionally externalized and must conform to this contract.

## 0.1 Document Scope and Governance

- This file defines the active runtime/API contract for the current milestone.
- Canonical documentation entrypoint is `docs/README.md`.
- Canonical runtime document set is:
  - `docs/README.md`
  - `docs/FEATURE_SPEC.md`
  - `docs/API_CONTRACT.md`

## 1. Client Surface

### Resource and Filesystem

- `initialize() -> Result<()>`
- `add_resource(path_or_url, target?, reason?, instruction?, wait, timeout?) -> AddResourceResult`
- `wait_processed(timeout?) -> QueueStatus`
- `ls(uri, recursive, simple) -> List<Entry>`
- `glob(pattern, uri?) -> GlobResult`
- `read(uri) -> String`
- `abstract_text(uri) -> String`
- `overview(uri) -> String`
- `mkdir(uri) -> Result<()>` (`extension`)
- `rm(uri, recursive) -> Result<()>`
- `mv(from_uri, to_uri) -> Result<()>`
- `tree(uri) -> TreeResult`
- `load_markdown(uri) -> MarkdownDocument` (`extension`)
- `save_markdown(uri, content, expected_etag?) -> MarkdownSaveResult` (`extension`, full-replace only)

Internal scope access (`extension`):

- `ls("axiom://temp", recursive?, simple?)`
- `ls("axiom://queue", recursive?, simple?)`

Restriction:

- `queue` scope is read-only for non-system operations.
- `wait_processed(timeout?)` is an active wait operation:
  - repeatedly replays due queue events;
  - returns when queue work is drained (`new_total == 0 && processing == 0`);
  - returns `CONFLICT` on timeout with queue counts in message.

Markdown web editor (`extension`):

- `axiomme web --host 127.0.0.1 --port 8787`
- CLI behavior: `axiomme web` performs explicit handoff to an external viewer binary (`AXIOMME_WEB_VIEWER_BIN` override, default `axiomme-webd`).
- Viewer implementation lives outside this repository and consumes this API contract.
- Startup gate (viewer): web server runs scoped reconciliation (`resources/user/agent/session`) and serves endpoints only on successful recovery.
- Web responses include security headers (`Content-Security-Policy`, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`).
- `GET /api/document?uri=axiom://... -> { uri, content, etag, updated_at, format, editable }`
  - Supported formats: `markdown`, `json`, `jsonl`, `yaml`, `xml`, `text`
  - `editable=true` for `markdown`, `json`, `yaml`
  - `editable=false` for `jsonl`, `xml`, `text`
- `POST /api/document/save { uri, content, expected_etag? } -> MarkdownSaveResult`
  - Save target supports `markdown`, `json`, `yaml`
  - Save path keeps full-replace + etag + sync reindex + rollback policy
  - Save reindex scope is targeted to the changed document and its ancestor tier chain
  - Unrelated invalid sibling paths do not fail save/reindex for the edited document
- `GET /api/markdown?uri=axiom://... -> MarkdownDocument`
- `POST /api/markdown/save { uri, content, expected_etag? } -> MarkdownSaveResult`
- `POST /api/markdown/preview { content } -> { html }`
- `GET /api/fs/list?uri=axiom://...&recursive=false -> { uri, entries }`
- `GET /api/fs/tree?uri=axiom://... -> TreeResult`
- `POST /api/fs/mkdir { uri } -> { status, uri }`
- `POST /api/fs/move { from_uri, to_uri } -> { status, from_uri, to_uri }`
- `POST /api/fs/delete { uri, recursive? } -> { status, uri }`
- Preview rendering sanitizes raw HTML input and blocks unsafe link/image URL schemes (`javascript:`, `data:`, etc.).

Markdown web error/status contract (`extension`):

- `409 CONFLICT`: stale `expected_etag`
- `423 LOCKED`: another save+reindex is in-flight
- `500 INTERNAL_ERROR`: may include rollback details
  - `details.reindex_err`
  - `details.rollback_write`
  - `details.rollback_reindex`

Markdown request metrics (`extension`):

- Request logs include:
  - `markdown.load`: `content_bytes`
  - `markdown.save`: `save_ms`, `reindex_ms`, `total_ms`, `content_bytes`, `reindexed_root`
  - `document.load`: `content_bytes`
  - `document.save`: `save_ms`, `reindex_ms`, `total_ms`, `content_bytes`, `reindexed_root`

### Retrieval

- `find(query, target_uri?, limit?, score_threshold?, filter?) -> FindResult`
- `find_with_budget(query, target_uri?, limit?, score_threshold?, filter?, budget?) -> FindResult` (`extension`)
- `search(query, target_uri?, session?, limit?, score_threshold?, filter?) -> FindResult`
- `search_with_budget(query, target_uri?, session?, limit?, score_threshold?, filter?, budget?) -> FindResult` (`extension`)
- `search_with_request(SearchRequest{ ..., runtime_hints }) -> FindResult` (`extension`)

Runtime hint boundary (`extension`):

- `SearchRequest.runtime_hints` is request-scoped context only.
- Runtime hints are not persisted to `messages.jsonl`.
- Runtime hints do not enqueue outbox events and are excluded from commit extraction input.

Ranking behavior (`extension`):

- Post-retrieval reranker is off by default and enabled only with `AXIOMME_RERANKER=doc-aware-v1`.
- Retrieval backend is memory-only (`AXIOMME_RETRIEVAL_BACKEND=memory`).
- Invalid `AXIOMME_RETRIEVAL_BACKEND` token is treated as configuration error (fail-fast).
- Retrieval query-plan notes include explicit backend policy marker:
  - `backend_policy:memory_only`
- Retrieval typed-edge enrichment is opt-in:
  - `AXIOMME_SEARCH_TYPED_EDGE_ENRICHMENT=1|true|yes|on`
  - when enabled, relation items may include:
    - `relation_type`
    - `source_object_type`
    - `target_object_type`
  - query-plan notes include:
    - `typed_edge_enrichment:1`
    - `typed_edge_links:<count>`
- Retrieval tokenization is deterministic in the memory search path.
- Search request logs include backend policy fields:
  - `retrieval_backend`
  - `retrieval_backend_policy`
  - `typed_edge_enrichment`

Embedding provider behavior (`extension`):

- `AXIOMME_EMBEDDER=semantic-lite|hash|semantic-model-http`
- `semantic-model-http` uses local HTTP embedding endpoint with:
  - `AXIOMME_EMBEDDER_MODEL_ENDPOINT`
  - `AXIOMME_EMBEDDER_MODEL_NAME`
  - `AXIOMME_EMBEDDER_MODEL_TIMEOUT_MS`
- `AXIOMME_EMBEDDER_STRICT=1|true|yes|on` enables strict mode:
  - semantic-model-http initialization/request/response failures are recorded as strict embedding errors;
  - benchmark report environment records `embedding_strict_error` for the run;
  - release-profile benchmark gate fails when latest report has `embedding_strict_error`.
- Endpoint host must be loopback (`127.0.0.1`, `localhost`, `::1`) to enforce local/offline policy.
- Release-profile benchmark gate (`gate_profile` contains `release` or `write_release_check=true`) requires benchmark report embedding provider `semantic-model-http`.
- Benchmark gate result may include structured embedding diagnostics:
  - `embedding_provider`
  - `embedding_strict_error`
- Benchmark gate options (`benchmark gate`) additionally support:
  - `min_stress_top1_accuracy` (optional)
- Benchmark gate result/release check may include:
  - `stress_top1_accuracy`
  - `min_stress_top1_accuracy`
- Release check document includes embedding diagnostics when available:
  - `embedding_provider`
  - `embedding_strict_error`

### Session

- `session(session_id?) -> SessionHandle`
- `sessions() -> List<SessionInfo>` (`extension`)
- `delete(session_id) -> bool` (`extension`)
- `promote_session_memories(request) -> MemoryPromotionResult` (`extension`)
- `checkpoint_session_archive_only(session_id) -> CommitResult` (`extension`)
- `promote_and_checkpoint_archive_only(request) -> MemoryPromotionResult` (`extension`)

### Ontology

- `ontology validate --uri?` validates and compiles schema contract.
- `ontology pressure --uri?` reports `OntologySchemaV2` escalation pressure from explicit thresholds.
- `ontology trend --history-dir --min-samples --consecutive-v2-candidate` evaluates trend-based escalation from snapshot history.
- `ontology action-validate --uri? --action-id --queue-event-type [--input-json|--input-file|--input-stdin]` validates action request contract without side effects.
- `ontology action-enqueue --uri? --target-uri --action-id --queue-event-type [--input-json|--input-file|--input-stdin]` validates action request contract and enqueues one outbox event.
- `ontology invariant-check --uri? [--enforce]` evaluates invariant rules and optionally enforces zero failures.
- Pressure report contract fields:
  - `schema_version`
  - `object_type_count`
  - `link_type_count`
  - `action_type_count`
  - `invariant_count`
  - `action_invariant_total`
  - `link_types_per_object_basis_points`
  - `v2_candidate`
  - `trigger_reasons[]`
  - `policy` (`min_action_types`, `min_invariants`, `min_action_invariant_total`, `min_link_types_per_object_basis_points`)
- Trend report contract fields:
  - `total_samples`
  - `consecutive_v2_candidate_tail`
  - `trigger_v2_design`
  - `status` (`insufficient_samples|monitor|trigger_v2_design`)
  - `policy` (`min_samples`, `consecutive_v2_candidate`)
  - `latest_sample_id`
  - `latest_generated_at_utc`
  - `latest_v2_candidate`
- Trend policy input constraints:
  - `min_samples >= 1`
  - `consecutive_v2_candidate >= 1`
- Action validate report contract fields:
  - `action_id`
  - `queue_event_type`
  - `input_contract`
  - `input_kind` (`null|boolean|number|string|array|object`)
- Action enqueue payload contract fields:
  - `schema_version`
  - `action_id`
  - `input` (JSON value)
- Action input source constraints:
  - at most one input source (`--input-json`, `--input-file`, `--input-stdin`)
  - when no explicit source is provided, input is `null`
- Action input contract checks:
  - recognized contracts: `json-any|json-null|json-boolean|json-number|json-string|json-array|json-object`
  - unknown contract strings are rejected during schema compile.
- Invariant check report contract fields:
  - `total`
  - `passed`
  - `failed`
  - `items[]`:
    - `id`, `severity`, `rule`, `message`
    - `status` (`pass|fail`)
    - `failure_kind?` (`invalid_severity|unsupported_rule|missing_target`)
    - `failure_detail?`
- Invariant rule grammar (v1 executable subset):
  - `object_type_declared:<object_type_id>`
  - `link_type_declared:<link_type_id>`
  - `action_type_declared:<action_type_id>`

Session handle:

- `load() -> Result<()>`
- `add_message(role, text) -> Message`
- `used(contexts?, skill?) -> Result<()>`
- `update_tool_part(message_id, tool_id, output, status?) -> Result<()>`
- `commit() -> CommitResult`
- `commit_with_mode(mode) -> CommitResult` (`extension`)
- `get_context_for_search(query, max_archives?, max_messages?) -> SearchContext`

Checkpoint promotion contract (`extension`):

- `commit_with_mode(archive_only)` archives active messages and skips auto memory extraction.
- `promote_session_memories` accepts explicit `MemoryPromotionRequest` facts only.
- Promotion idempotency key is `(session_id, checkpoint_id)` with deterministic `request_hash`.
- Same key + same hash returns cached result; same key + different hash returns validation conflict.
- In-flight same key (`phase=applying`) returns retryable conflict (`checkpoint_busy`).

### Package

- `export_ovpack(uri, to) -> String`
- `import_ovpack(file_path, parent, force, vectorize) -> String`

### Evidence and Release

- `run_security_audit(workspace_dir?) -> SecurityAuditReport` (`extension`)
- `collect_operability_evidence(trace_limit, request_limit) -> OperabilityEvidenceReport` (`extension`)
- `collect_reliability_evidence(replay_limit, max_cycles) -> ReliabilityEvidenceReport` (`extension`)
- `collect_release_gate_pack(options) -> ReleaseGatePackReport` (`extension`)

Release gate policy (`collect_release_gate_pack`) is evaluated as `G0..G8`:

- `G0` contract integrity:
  - executable contract probe test must pass (`axiomme-core` release contract probe)
  - `episodic` API probe test must pass (`axiomme-core` OM contract probe)
  - ontology contract probe test must pass (`axiomme-core` ontology contract probe)
  - ontology default schema contract must parse/compile and match required version (`schema.v1`)
  - ontology invariant check over compiled schema must have zero failures (`invariant_check_failed == 0`)
  - `crates/axiomme-core/Cargo.toml`의 `episodic` 의존은 semver `0.1.x` 계약을 유지해야 함
  - `Cargo.lock`의 `episodic` 엔트리는 crates.io registry source여야 함 (`registry+https://github.com/rust-lang/crates.io-index`)
- `G1` build quality: `cargo check --workspace`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
  - host command policy:
    - `AXIOMME_HOST_TOOLS=on|off` overrides host process execution.
    - default is target-driven (`on` for non-iOS, `off` for iOS).
    - when disabled, host-command-dependent checks return explicit `host_tools_disabled` failure details.
- `G2` reliability evidence: replay/recovery checks pass
- `G3` eval quality: `executed_cases > 0`, `top1_accuracy >= 0.75`, `filter_ignored == 0`, `relation_missing == 0`
- `G4` session memory: session probe passes with `memory_category_miss == 0`
- `G5` security audit:
  - dependency checks pass with `advisories_found == 0`
  - gate pass requires `mode=strict` (release-grade fresh advisory fetch)
  - `mode=offline` is allowed for local diagnostics but does not satisfy release gate
  - `mode=offline` does not fetch advisory data; strict bootstrap is required at least once per advisory DB path
  - advisory DB resolution order:
    - `AXIOMME_ADVISORY_DB` if set
    - else `<workspace>/.axiomme/advisory-db`
  - status fields are fixed enums:
    - `SecurityAuditReport.status`: `pass|fail`
    - `DependencyAuditSummary.mode`: `offline|strict`
    - `DependencyAuditSummary.status`: `passed|vulnerabilities_found|tool_missing|error|host_tools_disabled`
- `G6` benchmark gate: latency/accuracy/regression/quorum checks pass over benchmark history
  - strict release profile (`gate_profile` contains `release` or `write_release_check=true`) additionally requires `embedding_provider=semantic-model-http` and no strict embedding error
- `G7` operability evidence: trace/request-log evidence checks pass
- `G8` blocker rollup: pass only when unresolved blocker count is zero

## 2. Canonical Data Types

### FindResult

```json
{
  "memories": [{"uri":"axiom://...", "score":0.7, "abstract":"...", "relations":[{"uri":"axiom://...", "reason":"...", "relation_type":"depends_on", "source_object_type":"resource_doc", "target_object_type":"resource_doc"}]}],
  "resources": [{"uri":"axiom://...", "score":0.8, "abstract":"...", "relations":[{"uri":"axiom://...", "reason":"..."}]}],
  "skills": [{"uri":"axiom://...", "score":0.6, "abstract":"...", "relations":[{"uri":"axiom://...", "reason":"..."}]}],
  "query_plan": {},
  "query_results": []
}
```

Relation fields:

- `uri` (required)
- `reason` (required)
- `relation_type` (optional, typed-edge enrichment enabled only)
- `source_object_type` (optional, typed-edge enrichment enabled only)
- `target_object_type` (optional, typed-edge enrichment enabled only)

### CommitResult

```json
{
  "session_id": "abc123",
  "status": "committed",
  "memories_extracted": 3,
  "active_count_updated": 2,
  "archived": true,
  "stats": {
    "total_turns": 8,
    "contexts_used": 3,
    "skills_used": 1,
    "memories_extracted": 3
  }
}
```

### SearchRequest (extension)

```json
{
  "query": "runtime hints",
  "session": "s1",
  "runtime_hints": [
    {"kind":"observation", "text":"short-lived hint", "source":"episodic"},
    {"kind":"current_task", "text":"answer with boundary-safe flow", "source":"episodic"}
  ]
}
```

### MemoryPromotionRequest / Result (extension)

```json
{
  "session_id": "s1",
  "checkpoint_id": "cp-1",
  "apply_mode": "all_or_nothing",
  "facts": [
    {
      "category": "cases",
      "text": "Integrate episodic runtime boundary flow",
      "source_message_ids": ["m-1"],
      "source": "episodic",
      "confidence_milli": 850
    }
  ]
}
```

```json
{
  "session_id": "s1",
  "checkpoint_id": "cp-1",
  "accepted": 1,
  "persisted": 1,
  "skipped_duplicates": 0,
  "rejected": 0
}
```

Field semantics:

- `accepted`: validated incoming fact count after normalization/dedup.
- `persisted`: durable fact writes count (not unique file count).
- `skipped_duplicates`: dropped because same normalized category+text already existed.
- `rejected`: invalid or failed facts under `best_effort`.

### QueueStatus

```json
{
  "semantic": {
    "new_total": 2,
    "new_due": 1,
    "processing": 0,
    "processed": 10,
    "error_count": 0,
    "errors": []
  },
  "embedding": {
    "new_total": 1,
    "new_due": 0,
    "processing": 1,
    "processed": 4,
    "error_count": 1,
    "errors": []
  }
}
```

Notes:

- Lane counters are independent snapshots by lane (`semantic`, `embedding`) and are not mirrored.
- `QueueOverview` contains `counts` (global totals) plus `lanes` (`QueueStatus`) in the same response.
- `QueueOverview` and `QueueDiagnostics` include OM telemetry:
  - `queue_dead_letter_rate`: OM event dead-letter ratio by `event_type`
  - `om_status`: OM record/status counters (`observation_tokens_active`, buffering/reflecting counts, trigger totals)
  - `om_reflection_apply_metrics`: reflection apply counters (`attempts_total`, `stale_generation_total`, `stale_generation_ratio`) and latency (`avg_latency_ms`, `max_latency_ms`)

### ReleaseGateDecision / ReleaseGateDetails (extension)

`ReleaseGateDecision.details` is a tagged union:
`ReleaseGateDecision.gate_id` is a fixed enum code (`G0`..`G8`), not a free-form string.
`ReleaseGateDecision.status` and `ReleaseGatePackReport.status` are fixed enum values (`pass|fail`).
`details.data.audit_status` (`security_audit` kind) is a fixed enum value (`passed|vulnerabilities_found|tool_missing|error|host_tools_disabled`).
`ReleaseGateDecision.evidence_uri` is optional and omitted when unavailable.

```json
{
  "gate_id": "G0",
  "passed": true,
  "status": "pass",
  "details": {
    "kind": "contract_integrity",
    "data": {
      "policy": {
        "required_major": 0,
        "required_minor": 1,
        "required_lock_source_prefix": "registry+https://github.com/rust-lang/crates.io-index",
        "allowed_manifest_operators": ["exact", "caret", "tilde"]
      },
      "contract_probe": {"test_name":"...", "command_ok":true, "matched":true, "output_excerpt":"...", "passed":true},
      "episodic_api_probe": {"test_name":"...", "command_ok":true, "matched":true, "output_excerpt":"...", "passed":true},
      "episodic_semver_probe": {
        "passed": true,
        "error": null,
        "manifest_req": "0.1.0",
        "manifest_req_ok": true,
        "manifest_uses_path": false,
        "manifest_uses_git": false,
        "manifest_source_ok": true,
        "lock_version": "0.1.0",
        "lock_version_ok": true,
        "lock_source": "registry+https://github.com/rust-lang/crates.io-index",
        "lock_source_ok": true
      }
    }
  },
  "evidence_uri": null
}
```

Supported `details.kind` values:

- `contract_integrity` (`G0`): `{ policy, contract_probe, episodic_api_probe, episodic_semver_probe, ontology_policy?, ontology_probe? }`
- `build_quality` (`G1`): `{ cargo_check, cargo_fmt, cargo_clippy, check_output, fmt_output, clippy_output }`
- `reliability_evidence` (`G2`): `{ status, replay_done, dead_letter }`
- `eval_quality` (`G3`): `{ executed_cases, top1_accuracy, min_top1_accuracy, failed, filter_ignored, relation_missing }`
- `session_memory` (`G4`): `{ base_details, memory_category_miss }`
- `security_audit` (`G5`): `{ status, mode, strict_mode_required, strict_mode, audit_status, advisories_found, packages }`
- `benchmark` (`G6`): `{ passed, evaluated_runs, passing_runs, reasons }`
- `operability_evidence` (`G7`): `{ status, traces_analyzed, request_logs_scanned }`
- `blocker_rollup` (`G8`): `{ unresolved_blockers }`

## 3. Error Contract

```json
{
  "code": "INVALID_URI",
  "message": "invalid URI: axiom://invalid",
  "operation": "read",
  "uri": "axiom://invalid",
  "trace_id": "uuid-v4"
}
```

Required fields:

- `code`
- `message`
- `operation`
- `trace_id`

Optional fields:

- `uri`
- `details`

## 4. Stability

- This is a development-stage contract.
- Backward compatibility is not guaranteed between internal milestones.
- Contract fixtures are enforced in:
  - `crates/axiomme-core/tests/fixtures/core_contract_fixture.json`
  - `crates/axiomme-core/tests/core_contract_fixture.rs`
  - `crates/axiomme-core/tests/fixtures/release_contract_fixture.json`
  - `crates/axiomme-core/tests/release_contract_fixture.rs`
