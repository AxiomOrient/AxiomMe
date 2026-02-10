# API Contract

## 0. Policy

- Baseline behavior is defined by this document.
- Additive extensions are allowed and must be marked as `extension`.
- Canonical URI protocol is `axiom://`.

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

- Hybrid merge uses rank-based reciprocal fusion.
- Post-retrieval reranker profile is controlled by `AXIOMME_RERANKER` (`doc-aware-v1`, `off`).

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
  "semantic": {"processed": 10, "error_count": 0, "errors": []},
  "embedding": {"processed": 10, "error_count": 0, "errors": []}
}
```

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
