# Markdown Web Viewer/Edit Plan

Date: 2026-02-10  
Scope: Markdown only (`.md`, `.markdown`)  
Priority: correctness and consistency over latency/concurrency

## 0. Execution Status (2026-02-10)

- Phase M1: DONE (core load/save/etag/full-replace/rollback/reindex)
- Phase M2: DONE (local web API and explicit `409`/`423`/`500` mapping)
- Phase M3: DONE (editor+preview UI, save status, conflict/lock UX, shortcut save)
- Phase M4: DONE (startup recovery gate + save/reindex latency metrics + crash-adjacent regression tests + preview sanitization + security headers)
- Extension: DONE (unified `/api/document` viewer + `/api/document/save` for markdown/json/yaml, with read-only `jsonl/xml/txt`)

## 1. Final Judgment

For this project state and requirements, the best strategy is:

1. Save with **full document replace** only.
2. Run **synchronous reindex** immediately after save.
3. **Block** editor operations during save+reindex.
4. Use `etag` conflict checks to prevent silent overwrite.

Rationale:

1. Full replace has the smallest correctness surface.
2. Partial patch introduces more failure modes than benefits.
3. Synchronous reindex guarantees post-save query correctness.
4. Temporary blocking is acceptable and aligned with requirements.

## 2. Partial Patch vs Full Replace

### Option A: Partial Patch Save

Pros:

1. Smaller write payload.
2. Potentially lower write latency.

Risks:

1. Patch target drift (line/offset mismatch).
2. Incorrect merge under concurrent edits.
3. Partial save success with index mismatch.
4. Hard rollback semantics.

Assessment:

1. Higher implementation complexity.
2. Higher defect probability.
3. Not aligned with "accuracy first" mandate.

### Option B: Full Replace Save

Pros:

1. Deterministic write semantics.
2. Easy conflict/rollback model.
3. Simple index consistency rules.
4. Easy to test exhaustively.

Tradeoff:

1. Higher save latency for large files.

Assessment:

1. Best risk-adjusted choice for current phase.

## 3. Consistency Contract

Editor save must satisfy all invariants:

1. No silent overwrite (`etag` required for guarded save).
2. File content and search index move together as one logical commit.
3. Failed save must not leave ambiguous partial state.
4. Internal scopes (`temp`, `queue`) cannot be edited.
5. Tier artifacts (`.abstract.md`, `.overview.md`) cannot be edited directly.

## 4. Strict Save Pipeline

### 4.1 Locking Policy

During `save + reindex`:

1. Acquire process-local exclusive editor lock.
2. Block `load/save` API requests with `423 Locked` (or short wait then `423`).
3. Release lock only after success or compensated rollback.

Strictness mode:

1. Default is strict mode enabled.
2. In strict mode, editor API reads/writes are blocked during save+reindex.
3. If later needed, strict mode can be relaxed only after dedicated correctness proof.

Why block:

1. Guarantees single writer.
2. Prevents UI from reading transitional states.
3. Reduces race complexity to near zero.

### 4.2 Save Algorithm (Authoritative)

Input: `uri`, `content`, `expected_etag`  
Output: committed response or explicit failure

Steps:

1. Validate target URI/scope/extension/not-directory/not-tier-artifact.
2. Acquire editor lock.
3. Read current content and compute `current_etag`.
4. If `expected_etag` provided and mismatch, return `409 Conflict`.
5. Write new content atomically (`tmp -> fsync -> rename`).
6. Run synchronous `reindex_uri_tree(parent_uri)`.
7. If reindex succeeds, compute `new_etag`, return success.
8. If reindex fails, execute compensation rollback:
9. Restore previous content atomically.
10. Re-run synchronous reindex to restore previous index state.
11. Return `500` with rollback status (`rollback_applied: true/false`).
12. Release editor lock in all branches.

### 4.3 Crash Safety

Crash cases:

1. Crash before atomic rename:
2. Original file remains intact.
3. Crash after atomic rename before reindex:
4. On next startup, call `initialize()` then `reconcile_state()` before serving editor APIs.
5. Crash during rollback:
6. Startup recovery path must force reindex before editor endpoints are enabled.

## 5. API Design (Markdown Web)

### 5.1 Endpoints

1. `GET /` -> editor page
2. `GET /api/markdown?uri=...` -> load markdown
3. `POST /api/markdown/save` -> full replace save
4. `POST /api/markdown/preview` -> rendered preview

### 5.2 Save Request/Response

Request:

1. `uri: string`
2. `content: string` (full document)
3. `expected_etag: string | null`

Success response:

1. `uri`
2. `etag`
3. `updated_at`
4. `reindexed_root`
5. `save_ms`
6. `reindex_ms`

Error codes:

1. `400` invalid input/uri/extension
2. `403` forbidden scope or forbidden target file
3. `404` not found
4. `409` etag conflict
5. `423` editor lock active
6. `500` internal failure (includes rollback outcome)

## 6. Scenario Matrix

### S1: Single editor, normal save

Expected:

1. Save succeeds.
2. Reindex succeeds.
3. New content is searchable immediately.

### S2: Two tabs, stale etag in tab B

Expected:

1. Tab A saves first.
2. Tab B save returns `409`.
3. No silent overwrite.

### S3: Save request during in-flight reindex

Expected:

1. Request blocked (`423` or bounded wait then `423`).
2. No concurrent commit.

### S4: Invalid target (`axiom://queue/...`)

Expected:

1. Save rejected with `403`.
2. No write or reindex attempted.

### S5: Tier file edit attempt (`.overview.md`)

Expected:

1. Save rejected with `403`.
2. Source document editing path only.

### S6: Reindex failure after file replace

Expected:

1. Automatic rollback to previous file content.
2. Restore reindex attempted.
3. Response includes rollback status.

### S7: Process crash after replace, before reindex

Expected:

1. Startup recovery runs (`initialize + reconcile`) before serving editor APIs.
2. Index/file consistency restored.

### S8: Large markdown file save

Expected:

1. Operation is slower but correct.
2. Lock duration observable in metrics.

## 7. Implementation Plan

### Phase M1: Core Consistency Engine

Tasks:

1. Add markdown load/save DTOs.
2. Add save service with lock + etag + full replace.
3. Add atomic write helper.
4. Add compensation rollback logic.
5. Add strict target validation.

Exit criteria:

1. Unit tests for S1, S2, S4, S5, S6 pass.

### Phase M2: Local Web API

Tasks:

1. Add `axiomme web` command.
2. Implement load/save/preview endpoints.
3. Surface structured error codes (`409`, `423`, `500 rollback status`).

Exit criteria:

1. Integration tests for S1, S2, S3 pass.

### Phase M3: Web UI

Tasks:

1. Editor + preview split view.
2. Save status UI (`saving/saved/conflict/locked/error`).
3. Conflict UX with reload option.
4. Shortcut save (`Ctrl/Cmd+S`).

Exit criteria:

1. Manual E2E flows complete on 3+ markdown docs.

### Phase M4: Recovery and Hardening

Tasks:

1. Startup recovery gate before web serve.
2. Save/reindex latency metrics.
3. Regression tests for crash-adjacent scenarios (simulated fault injection).

Exit criteria:

1. S7 and S8 validated.
2. `fmt`, `clippy -D warnings`, `test` all green.

## 8. Verification Checklist

Before declaring done:

1. `bash scripts/check_prohibited_tokens.sh`
2. `cargo fmt --all -- --check`
3. `cargo clippy --workspace --all-targets -- -D warnings`
4. `cargo test --workspace`
5. Manual browser flow:
6. Load -> edit -> save -> find reflects change.
7. Conflict flow returns `409`.
8. In-flight lock returns `423`.

## 9. Non-Goals (This Slice)

1. YAML/JSON/XML editing.
2. Partial patch/diff merge editing.
3. Real-time collaborative editing.
4. Binary editing.

## 10. Done Definition

Complete when all are true:

1. Markdown is viewable/editable in browser.
2. Save is full replace only.
3. Save executes sync reindex with lock.
4. Conflict and lock paths are explicit and tested.
5. Reindex failure compensation is implemented and tested.
6. Search reflects committed document state immediately after save.
