# AxiomMe Refactoring Task Plan

## Phase 1: Search Performance Optimization (Zero-Copy Search)
**Objective:** Eliminate `String` allocations in the hot search loop by modifying `ScoredRecord` and `IndexChildRecord` to use `Arc<str>`.

**Status (2026-02-27):** DONE  
**Evidence:** `TASK-015` in `docs/TASKS.md`, `cargo test -p axiomme-core --lib`

### Task 1.1: Update `ScoredRecord` and `IndexChildRecord`
- **Target:** `crates/axiomme-core/src/index.rs`
- **Action:** Change the type of the `uri` field in `ScoredRecord` and `IndexChildRecord` from `String` to `Arc<str>`.
- **Action:** Update the `search`, `children_of`, and related methods in `InMemoryIndex` to return `Arc<str>` (by cloning the `Arc` instead of `.to_string()`).

### Task 1.2: Update Downstream Consumers
- **Target:** `crates/axiomme-core/src/retrieval/engine.rs` (and callers like `AxiomMe::search`)
- **Action:** Update `TracePoint` (and potentially `ContextHit` if it clones strings unnecessarily) to accept `Arc<str>` or convert cleanly. 
- **Action:** Ensure the JSON serialization logic handles `Arc<str>` correctly (which Serde does natively).
- **Validation:** Run `cargo bench` or `cargo test` on retrieval module to ensure no semantic changes and verify compile-time lifetime/type safety.

## Phase 2: Decouple Domain Logic from LocalContextFs
**Objective:** Make `LocalContextFs` a pure byte/string I/O layer by moving ontology and relation logic to higher-level controllers.

**Status (2026-02-27):** DONE  
**Evidence:** `TASK-016` in `docs/TASKS.md`, `crates/axiomme-core/src/relation_documents.rs`, `crates/axiomme-core/src/tier_documents.rs`, `cargo test --workspace`

### Task 2.1: Move Relation Logic
- **Target:** `crates/axiomme-core/src/fs.rs` -> `crates/axiomme-core/src/client/relation.rs`
- **Action:** Extract `read_relations`, `write_relations`, and `validate_relations` from `LocalContextFs`.
- **Action:** Move this logic to `AxiomMe::read_relations` / `write_relations` (or within the `relation.rs` handler), using raw `fs.read()` and `fs.write()`.
- **Action:** Ensure `.relations.json` path generation is handled by the domain layer, not `fs.rs`.
- **Result:** moved to `crates/axiomme-core/src/relation_documents.rs` and consumed by `relation.rs`.

### Task 2.2: Move Tier/Markdown Logic
- **Target:** `crates/axiomme-core/src/fs.rs` -> `crates/axiomme-core/src/client/indexing.rs`
- **Action:** Extract `write_tiers`, `read_abstract`, and `read_overview` from `LocalContextFs`.
- **Action:** Move the logic to generate paths (`.abstract.md`, `.overview.md`) into the domain layer (e.g., as helper functions on `AxiomUri`).
- **Result:** moved to `crates/axiomme-core/src/tier_documents.rs` and consumed by `indexing.rs`, `resource.rs`, and `session/*`.

## Phase 3: Flatten Services and Fix CLI Leaks
**Objective:** Remove OOP-style `_service` suffixes and pull domain orchestration out of the CLI.

**Status (2026-02-27):** DONE  
**Evidence:** `TASK-018` in `docs/TASKS.md`, `crates/axiomme-core/src/client/ontology.rs`, renamed `crates/axiomme-core/src/client/*.rs`, `cargo test --workspace`

### Task 3.1: Encapsulate Ontology Enqueue Logic
- **Target:** `crates/axiomme-cli/src/commands/handlers.rs` (or `mod.rs`) & `crates/axiomme-core/src/client/om_bridge.rs` (or new `ontology.rs`)
- **Action:** Create `AxiomMe::enqueue_ontology_action(uri, target_uri, action_id, input_value)` in the core crate.
- **Action:** Refactor `handle_ontology_command` in the CLI to simply call this new core method. The core method should handle reading the schema, compiling it, validating the action, and pushing to the SQLite outbox.
- **Result:** added `AxiomMe::enqueue_ontology_action` in `crates/axiomme-core/src/client/ontology.rs`; `OntologyCommand::ActionEnqueue` now delegates orchestration to core API from CLI.

### Task 3.2: Rename Service Modules
- **Target:** `crates/axiomme-core/src/client/*.rs`
- **Action:** Rename modules to remove the `_service` suffix (e.g., `relation_service.rs` -> `relation.rs`, `indexing_service.rs` -> `indexing.rs`).
- **Action:** Update the `mod.rs` (or `client.rs`) module tree declarations.
- **Result:** top-level client modules/files renamed to non-`_service` names (`indexing`, `markdown_editor`, `mirror_outbox`, `om_bridge`, `queue_reconcile`, `relation`, `request_log`, `resource`, `runtime`) and module wiring updated in `client.rs`.

## Phase 4: Eradicate Magic Strings
**Objective:** Introduce type safety for queue statuses and pressure triggers.

**Status (2026-02-27):** DONE  
**Evidence:** `TASK-019`, `TASK-020` in `docs/TASKS.md`; `crates/axiomme-core/src/models/queue.rs`; `crates/axiomme-core/src/state/queue.rs`; `crates/axiomme-core/src/ontology/pressure.rs`; `cargo test --workspace`

### Task 4.1: Strongly Type Queue Status
- **Target:** `crates/axiomme-core/src/state/queue.rs` & `models/mod.rs`
- **Action:** Define `enum QueueEventStatus { New, Processing, Done, DeadLetter }`.
- **Action:** Implement `Display` and `FromStr` (or direct SQL mapping) for the enum.
- **Action:** Refactor `SqliteStateStore::enqueue` and `fetch_outbox` to accept/return this enum instead of raw string literals.
- **Result (2026-02-27):** DONE. Added `QueueEventStatus` enum and migrated outbox enqueue/fetch/mark APIs plus client/test call sites to typed status values.

### Task 4.2: Structured Pressure Triggers
- **Target:** `crates/axiomme-core/src/ontology/pressure.rs`
- **Action:** Define an enum `OntologyPressureTrigger` with variants like `ActionThresholdExceeded { current: usize, limit: usize }`.
- **Action:** Update `evaluate_v2_pressure` to return `Vec<OntologyPressureTrigger>` instead of formatted strings. Implement a serialization helper to format them as strings for API consumers if necessary.
- **Result (2026-02-27):** DONE. Added typed `OntologyPressureTrigger` variants, moved pressure/trend structs to typed trigger vectors, and preserved `trigger_reasons[]` JSON string compatibility with custom serde + unknown-string fallback.
