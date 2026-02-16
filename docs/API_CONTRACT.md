# API Contract

## 0. Policy

- Baseline behavior is defined by this document.
- Additive extensions are allowed and must be marked as `extension`.
- Canonical URI protocol is `axiom://`.
- Runtime is self-contained in this repository; OM domain logic is provided by `axiomme-core::om`.

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
- Startup gate: web server runs scoped reconciliation (`resources/user/agent/session`) and serves endpoints only on successful recovery.
- Web responses include security headers (`Content-Security-Policy`, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`).
- `GET /api/document?uri=axiom://... -> { uri, content, etag, updated_at, format, editable }`
  - Supported formats: `markdown`, `json`, `jsonl`, `yaml`, `xml`, `text`
  - `editable=true` for `markdown`, `json`, `yaml`
  - `editable=false` for `jsonl`, `xml`, `text`
- `POST /api/document/save { uri, content, expected_etag? } -> MarkdownSaveResult`
  - Save target supports `markdown`, `json`, `yaml`
  - Save path keeps full-replace + etag + sync reindex + rollback policy
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

Ranking behavior (`extension`):

- Post-retrieval reranker profile is controlled by `AXIOMME_RERANKER` (`doc-aware-v1`, `off`).
- Retrieval backend selection remains explicit via `AXIOMME_RETRIEVAL_BACKEND=sqlite|memory`.
- Invalid `AXIOMME_RETRIEVAL_BACKEND` token is treated as configuration error (fail-fast).
- SQLite lexical retrieval applies optional query normalization/alias expansion before FTS `MATCH`.
  - env: `AXIOMME_QUERY_NORMALIZER` (default enabled; set `off|none|0|false` to disable)

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

Session handle:

- `load() -> Result<()>`
- `add_message(role, text) -> Message`
- `used(contexts?, skill?) -> Result<()>`
- `update_tool_part(message_id, tool_id, output, status?) -> Result<()>`
- `commit() -> CommitResult`
- `get_context_for_search(query, max_archives?, max_messages?) -> SearchContext`

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
- `G1` build quality: `cargo check --workspace`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`
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
- `G6` benchmark gate: latency/accuracy/regression/quorum checks pass over benchmark history
  - strict release profile (`gate_profile` contains `release` or `write_release_check=true`) additionally requires `embedding_provider=semantic-model-http` and no strict embedding error
- `G7` operability evidence: trace/request-log evidence checks pass
- `G8` blocker rollup: pass only when unresolved blocker count is zero

## 2. Canonical Data Types

### FindResult

```json
{
  "memories": [{"uri":"axiom://...", "score":0.7, "abstract":"...", "relations":[{"uri":"axiom://...", "reason":"..."}]}],
  "resources": [{"uri":"axiom://...", "score":0.8, "abstract":"...", "relations":[{"uri":"axiom://...", "reason":"..."}]}],
  "skills": [{"uri":"axiom://...", "score":0.6, "abstract":"...", "relations":[{"uri":"axiom://...", "reason":"..."}]}],
  "query_plan": {},
  "query_results": []
}
```

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

## 3. Error Contract

```json
{
  "code": "INVALID_URI",
  "message": "Invalid axiom URI",
  "operation": "read",
  "uri": "axiom://invalid",
  "trace_id": "uuid"
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
