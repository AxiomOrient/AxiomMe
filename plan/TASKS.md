# Task Breakdown

Status legend: `TODO`, `IN_PROGRESS`, `BLOCKED`, `DONE`

## Execution Board

| ID | Scope | Status | Exit Criteria |
|---|---|---|---|
| AX-T001 | Canonical docs cleanup and conflict removal | DONE | Only active docs/plan set remains and is internally consistent |
| AX-T002 | New implementation plan + task + gates alignment | DONE | `plan/IMPLEMENTATION_PLAN.md`, `plan/TASKS.md`, `plan/QUALITY_GATES.md` are synchronized |
| AX-T003 | Prohibited-token scan policy | DONE | Automated scan blocks prohibited legacy tokens in code/docs/tests |
| AX-T101 | URI protocol migration to `axiom://` | DONE | Parser, formatter, storage mapping, CLI examples use `axiom://` only |
| AX-T102 | Repository-wide naming purge | DONE | Prohibited legacy naming removed from symbols, comments, docs, log templates |
| AX-T201 | Replacement-equivalence verification | DONE | Equivalence matrix rows are `DONE` and non-equivalent replacements are downgraded with owner/deadline |
| AX-T301 | Embedding architecture redesign | DONE | Pluggable embedding interface + production semantic backend + fallback strategy |
| AX-T302 | Embedding quality gate activation | DONE | Benchmark gate enforces semantic quality regression limits and quality suites are active in tests |
| AX-T401 | Retrieval correctness remediation | DONE | backend filter parity, result-limit correctness, budget exposure fixed |
| AX-T501 | Legacy implementation pruning | DONE | Legacy compatibility branches removed and canonical-only contracts are enforced |

## Phase Details

### Phase P0: Documents First

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-P0-01 | Remove obsolete/duplicate docs | - | DONE | Legacy docs removed and index updated |
| AX-P0-02 | Freeze canonical docs set | AX-P0-01 | DONE | `docs/README.md` lists only active docs |
| AX-P0-03 | Encode protocol/naming constraints in docs | AX-P0-02 | DONE | `axiom://` and naming rules reflected in feature/contract docs |

### Phase P1: Protocol and Naming Migration

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-P1-01 | Replace URI scheme constants and parser rules | AX-P0-03 | DONE | All URI parse/format paths output `axiom://` |
| AX-P1-02 | Migrate filesystem/state/index payload schema strings | AX-P1-01 | DONE | Persisted records use new protocol without mixed outputs |
| AX-P1-03 | Migrate CLI defaults/help/examples | AX-P1-01 | DONE | CLI UX emits only canonical protocol |
| AX-P1-04 | Rename prohibited legacy symbols/text | AX-P1-02 | DONE | Source/docs/tests/log templates pass token scan |
| AX-P1-05 | Add CI guard for prohibited tokens | AX-P1-04 | DONE | CI fails on prohibited-token matches |

### Phase P2: Replacement-Equivalence Audit

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-P2-01 | Define equivalence matrix for replacement paths | AX-P1-05 | DONE | Matrix covers behavior, failure mode, observability |
| AX-P2-02 | Validate ingest-finalize replacement behavior | AX-P2-01 | DONE | Equivalence tests pass on normal/error/restart cases |
| AX-P2-03 | Validate tier-generation replacement behavior | AX-P2-01 | DONE | L0/L1 correctness and drift checks pass |
| AX-P2-04 | Validate replay/reconcile replacement behavior | AX-P2-01 | DONE | Recovery invariants hold under fault injection |
| AX-P2-05 | Reclassify weak replacements to explicit TODO | AX-P2-01 | DONE | Any non-equivalent replacement is downgraded with owner and deadline |

### Phase P3: Embedding Reliability Fix

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-P3-01 | Introduce `Embedder` trait + provider abstraction | AX-P1-05 | DONE | Hash fallback and semantic provider share one interface |
| AX-P3-02 | Integrate local semantic embedding backend | AX-P3-01 | DONE | semantic-lite is the default local profile, hash fallback remains configurable, and functional/performance smoke tests pass |
| AX-P3-03 | Add vector-versioning and reindex migration | AX-P3-02 | DONE | Index profile stamp is persisted and mismatch enforces full reindex; Qdrant payload carries vector version metadata |
| AX-P3-04 | Add quality regression suite for retrieval metrics | AX-P3-02 | DONE | Benchmark gate enforces semantic quality regression (nDCG@10/Recall@10) <= 3% on eligible query sets |
| AX-P3-05 | Keep deterministic fallback with explicit policy | AX-P3-01 | DONE | Fallback behavior tested and bounded |
| AX-P3-06 | Add semantic tier synthesis backend option | AX-P2-05 | DONE | Directory tier synthesis supports semantic mode while keeping deterministic fallback |

### Phase P4: Retrieval Correctness Hardening

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-P4-01 | Fix backend MIME-filter parity | AX-P1-02 | DONE | Filter behavior consistent across backends |
| AX-P4-02 | Fix backend merge/limit truncation logic | AX-P1-02 | DONE | Result count honors request limit across modes |
| AX-P4-03 | Expose search budget in API and CLI | AX-P1-03 | DONE | Budget knobs available and trace stop reasons validated |
| AX-P4-04 | Add optional reranker extension and benchmark validation | AX-P2-05 | DONE | Reranker hook is measurable via benchmark deltas without breaking limit/filter guarantees |

### Phase P5: Legacy Pruning

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-P5-01 | Remove dead code and stale artifacts | AX-P4-03 | DONE | Legacy alias APIs/docs and obsolete compatibility docs are removed |
| AX-P5-02 | Update tests to canonical naming/protocol only | AX-P5-01 | DONE | Legacy JSON-shape acceptance tests replaced with canonical-shape enforcement tests |
| AX-P5-03 | Final cleanup and lock release branch | AX-P5-02 | DONE | All quality gates pass and blocker count is zero |

### Phase M: Markdown Web Viewer/Edit

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-M1-01 | Core markdown load/save consistency engine | AX-P5-03 | DONE | Full-replace + etag + sync reindex + rollback tests pass |
| AX-M2-01 | Local web API for markdown load/save/preview | AX-M1-01 | DONE | `GET /api/markdown`, `POST /api/markdown/save`, `POST /api/markdown/preview` implemented |
| AX-M2-02 | Explicit lock/conflict/error status mapping | AX-M2-01 | DONE | `409`, `423`, `500` responses are deterministic and tested |
| AX-M3-01 | Web UI editor/preview + save UX | AX-M2-01 | DONE | Split editor/preview, save status, reload path, `Ctrl/Cmd+S` |
| AX-M4-01 | Recovery gate and hardening | AX-M3-01 | DONE | Startup reconciliation gate before serve, markdown save/reindex latency metrics in request logs, crash-adjacent regression tests, preview sanitization for raw HTML and unsafe URL schemes, and baseline web security headers |

### Phase V: Multi-Format Document Viewer

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-V1-01 | Add unified document load endpoint | AX-M4-01 | DONE | `GET /api/document` returns `{ uri, content, etag, updated_at, format, editable }` for supported formats |
| AX-V1-02 | Add read-only web viewer mode for non-editable formats | AX-V1-01 | DONE | UI disables save for `jsonl/xml/txt` and renders escaped read-only preview |
| AX-V1-03 | Enforce format guardrails in viewer API | AX-V1-01 | DONE | Unsupported extensions return deterministic validation error and are covered by tests |

### Phase V2: Structured Document Editing

| ID | Task | Depends On | Status | Done Criteria |
|---|---|---|---|---|
| AX-V2-01 | Extend document editor policy beyond markdown | AX-V1-01 | DONE | Core document editor supports `.json/.yaml/.yml` with same full-replace/etag/reindex/rollback semantics |
| AX-V2-02 | Add `/api/document/save` endpoint | AX-V2-01 | DONE | Web API supports markdown/json/yaml save with deterministic error mapping |
| AX-V2-03 | Add structured content validation | AX-V2-01 | DONE | JSON/YAML invalid payloads are rejected with `VALIDATION_FAILED` and covered by tests |
