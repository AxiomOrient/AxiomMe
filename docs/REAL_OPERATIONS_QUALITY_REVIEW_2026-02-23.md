# Real Operations Quality Review (2026-02-23)

## Goal

Validate real-world operability (not only tests) for:

1. title-driven retrieval quality
2. document CRUD workflows
3. ontology contract workflow
4. relation CRUD and retrieval enrichment
5. queue execution path

Data set:

- `/Users/axient/Documents/contextSet`

Operational roots:

- `/tmp/axiomme-real-ops-20260223-v2`
- `/tmp/axiomme-real-ops-20260223-v3`

Evidence output:

- `/tmp/axiomme-real-ops-20260223-v2-out`
- `/tmp/axiomme-real-ops-20260223-v3-out`

## Quality Findings Before Fix

1. `find "File Structure (Lean Architecture)"` did not return `FILE_STRUCTURE.md` as top1.
2. `find "아키텍트 페르소나"` failed to return the expected architect profile document in global scope.
3. Relation ownership model was implicit. A relation file placed under an unrelated owner path could link docs, but enrichment semantics were ambiguous.

## Changes Applied

### 1) Retrieval score model made explicit for title exact-match

File:

- `crates/axiomme-core/src/index.rs`

Changes:

1. Added explicit `exact` signal to `ScoredRecord`.
2. Added `ExactRecordKeys` cache that now includes:
   - `name`
   - `uri basename/stem`
   - `abstract_text` (document title tier)
3. Added exact matching rules for full raw match and token-signature match.
4. Kept score composition explicit via weight constants (`W_EXACT`, `W_DENSE`, `W_SPARSE`, `W_RECENCY`, `W_PATH`).

### 2) Relation ownership invariant made explicit

File:

- `crates/axiomme-core/src/client/relation_service.rs`

Changes:

1. Explicit permission guard for `queue` scope writes.
2. Enforced invariant:
   - every linked URI must be inside `owner_uri` subtree.
3. Preserved ontology validation semantics by running ontology link validation before subtree ownership check.

### 3) CLI relation boundary made first-class and explicit

Files:

- `crates/axiomme-cli/src/cli/relation.rs`
- `crates/axiomme-cli/src/cli/mod.rs`
- `crates/axiomme-cli/src/commands/mod.rs`
- `crates/axiomme-cli/src/commands/tests.rs`
- `crates/axiomme-cli/src/cli/tests.rs`

Changes:

1. Added `relation list|link|unlink`.
2. Added preflight check: `relation link` requires at least two `--uri`.
3. Added parser/runtime tests for relation commands.

## Test and Verification

### Static and regression checks

1. `cargo fmt --all`
2. `cargo check --workspace`
3. `cargo clippy --workspace --all-targets -- -D warnings`
4. `cargo test --workspace`

All passed.

### Real operations checks (v3)

Summary:

1. `find "File Structure (Lean Architecture)"` -> expected doc rank 1.
2. `find "ContextWorks - System Overview"` -> expected doc rank 1.
3. `find "아키텍트 페르소나"` (global target) -> expected doc rank 1.
4. `find "FILE_STRUCTURE.md"` -> expected doc rank 1.
5. relation enrichment observed for linked document.
6. invalid relation ownership (`owner_uri` outside linked URI subtree) fails with non-zero exit.
7. ontology action enqueue + queue work path completed.

## Self-Feedback Loop Notes

1. During validation, `document save --mode markdown` on a non-existent file failed.
   - This was not a regression in this patch; it reflects current editor contract.
   - Real CRUD workflow was adjusted to create files via `add` then mutate via `document save`.
2. While introducing relation ownership invariant, initial ordering caused two existing tests to fail by changing error precedence.
   - Fixed by explicit queue permission guard and preserving ontology validation precedence.

## Remaining Risk (Non-blocking)

1. Markdown editor create semantics are still strict (`save` requires existing target).
2. For operators, this can feel unintuitive for first-write workflows.
3. If needed, add an explicit `document create` command rather than weakening `save` semantics.

## Random Real-Use Validation (v4, binary path)

Date:

- 2026-02-23

Operational root:

- `/tmp/axiomme-realbench-bin-1771861338`

Evidence output:

- `/tmp/axiomme-realbench-bin-1771861338-out/summary.json`
- `/tmp/axiomme-realbench-bin-1771861338-out/samples.jsonl`

Method:

1. Built `target/debug/axiomme-cli` once.
2. Ingested `/Users/axient/Documents/contextSet` with `--markdown-only`.
3. Randomly sampled 60 markdown files (effective sample count: 51 after compact-query length filter).
4. Ran 3 query modes per sample:
   - `title` (first heading or file stem)
   - `compact` (alnum-only query)
   - `typo` (single-char substitution on compact query tail)

Result:

1. `title` top1 accuracy: `45/51 = 0.8823`, `p95=1422ms`
2. `compact` top1 accuracy: `32/51 = 0.6274`, `p95=1410ms`
3. `typo` top1 accuracy: `9/51 = 0.1764`, `p95=1412ms`

Interpretation:

1. Title-based retrieval is operationally stable for real context documents.
2. Compact/punctuationless query support improved and is now usable.
3. Single-char typo support works for a subset of cases but remains a quality gap for mixed-language corpora.

Change tied to this validation:

1. Added compact key + edit-distance(<=1) exact-boost path in:
   - `crates/axiomme-core/src/index.rs`
2. Added deterministic regression tests:
   - `compact_key_exact_match_handles_punctuationless_query`
   - `compact_key_edit_distance_one_prioritizes_filename_typo`
3. Reduced per-search recency scoring overhead by evaluating `Utc::now()` once per search call.

## Release-Binary Spot Check (v5)

Date:

- 2026-02-23

Operational root:

- `/tmp/axiomme-realbench-rel-1771861812`

Evidence output:

- `/tmp/axiomme-realbench-rel-1771861812-out/summary.json`
- `/tmp/axiomme-realbench-rel-1771861812-out/samples.jsonl`

Method:

1. Built `target/release/axiomme-cli`.
2. Ingested `/Users/axient/Documents/contextSet`.
3. Random sample 20 documents.
4. Title query top1 check only.

Result:

1. Top1 accuracy: `19/20 = 0.95`
2. End-to-end process p95 (single CLI invocation): `224ms`

## Limit-Sensitivity Regression and Fix (v6)

Date:

- 2026-02-23

Operational root:

- `/tmp/axiomme-realops-1771868005`

Evidence output:

- `/tmp/axiomme-realops-bench-1771868188/summary.json` (pre-fix)
- `/tmp/axiomme-realops-bench-1771868188/cases.ndjson` (pre-fix cases)
- `/tmp/axiomme-realops-bench-1771868552/summary.json` (post-fix)
- `/tmp/axiomme-realops-bench-1771868552/cases.ndjson` (post-fix cases)
- `/tmp/axiomme-realops-bench-1771868552/trials.json` (3 random-seed trials)

Issue found:

1. For some title queries, `--limit 1` returned a non-optimal result while `--limit 10` returned the expected document at rank 1.
2. Root cause was DRR early convergence bias: when a single branch stabilized quickly, final output could exclude better global candidates.

Applied fix:

1. File: `crates/axiomme-core/src/retrieval/expansion.rs`
2. Always merge a small leaf-only global-rank baseline into final candidate set before truncate.
3. Keep max score per URI while merging (no hidden overwrite).
4. Fast-path trace metrics are now explicit and real (`latency_ms`, `explored_nodes`) instead of fixed zeros.

Regression test added:

1. File: `crates/axiomme-core/src/retrieval/tests.rs`
2. Test: `drr_small_limit_preserves_global_exact_candidate`
3. Asserts that with small node budget and `limit=1`, exact global candidate remains top1.

Self-feedback and correction log:

1. Initial benchmark script used a space-delimited cut and could truncate paths containing spaces.
2. Fixed sampling pipeline to use tab delimiter and re-ran the full benchmark.
3. First implementation merged directory records and caused one contract test failure.
4. Corrected to merge leaf records only; full suite revalidated.

Post-fix result snapshot (same real dataset):

1. `stem` top1: `1.0` (`27/27`)
2. `typo_append` top1: `1.0` (`27/27`)
3. `title` top1: `1.0` (`27/27`)
4. Additional 3 seeded random trials (20 files each) also reported `1.0` for stem/typo/title across sampled cases.

## Index Allocation Review (v7)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`

Issue found:

1. Fuzzy bigram scoring cloned and re-sorted candidate vectors on each comparison.
2. This created avoidable heap allocations on the hot retrieval path.

Applied fix:

1. `compact_char_bigrams` now sorts once during key construction.
2. `sorensen_dice_multiset` now runs merge-only on pre-sorted slices (no per-call clone/sort).
3. Added debug assertions to enforce sorted-input invariant.
4. Replaced repeated `query.compact_key.chars().count()` with cached `query.compact_len`.

Regression checks:

1. `compact_char_bigrams_are_sorted_for_merge_scoring`
2. `sorensen_dice_multiset_counts_duplicates`
3. Full `axiomme-core` + `axiomme-cli` tests and workspace clippy passed.

Operational spot check:

1. `/tmp/axiomme-realops-bench-idxopt-1771893705.json`
2. sampled stem queries top1: `1.0` (`18/18`)
3. average latency: `2.39ms`

## Planner Data-Model Simplification (v8)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/retrieval/planner.rs`

Issue found:

1. Scope normalization/dedup used temporary string maps.
2. Query dedup key used formatted string concatenation and join-based scope signatures.
3. Scope listing for query plans allocated strings before dedup.

Applied fix:

1. Scope normalization is now value-based:
   - sort `Scope` by `as_str()`
   - dedup directly on `Scope` values
2. Query dedup key is now typed data:
   - `(query_lower, Vec<Scope>)`
3. Scope names collection now dedups `Scope` first, then materializes strings once.

Regression checks:

1. Added planner unit tests:
   - `normalize_scopes_is_value_based_and_sorted`
   - `dedup_queries_ignores_scope_order_after_normalization`
   - `collect_scope_names_returns_sorted_distinct_names`
2. `axiomme-core` full tests passed.
3. `axiomme-cli` full tests passed.
4. workspace clippy (`-D warnings`) passed.

Self-feedback and correction log:

1. While running targeted retrieval tests, I initially passed multiple test filters in one `cargo test` invocation and got a CLI argument error.
2. Re-ran each filter explicitly and then revalidated with full suite commands.

## Retrieval Merge/Convergence Allocation Trim (v9)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/retrieval/scoring.rs`
- `crates/axiomme-core/src/retrieval/expansion.rs`

Issue found:

1. Merge paths (`merge_hits`, leaf-hit upsert, baseline merge) cloned `ContextHit` values during max-score reconciliation.
2. Convergence update cloned all selected hits each round (`selected.values().cloned()`) before sorting top-k URIs.
3. Equal-score top-k ordering depended on hash-map iteration order (implicit, non-deterministic tie behavior).

Applied fix:

1. Replaced clone-based merge with ownership-based upsert:
   - introduced explicit `upsert_hit_if_higher(...)` in expansion path
   - rewrote `merge_hits(...)` and `merge_trace_points(...)` with `get_mut` + insert-once flow
2. Rewrote convergence top-k evaluation to sort references (`Vec<&ContextHit>`) instead of cloned records.
3. Added explicit tie-break (`uri` lexical order) for deterministic equal-score top-k behavior.
4. Minor allocation polish:
   - `typed_query_plans(...)` now preallocates with `Vec::with_capacity`
   - start-point/frontier URI reuse reduces duplicate clone points in frontier initialization

Regression checks:

1. Added tests:
   - `retrieval::expansion::tests::upsert_hit_if_higher_keeps_max_score_per_uri`
   - `retrieval::expansion::tests::convergence_topk_is_deterministic_for_equal_scores`
2. Full validation passed:
   - `cargo fmt --all`
   - `cargo test -p axiomme-core`
   - `cargo test -p axiomme-cli`
   - `cargo clippy --workspace --all-targets -- -D warnings`

Self-feedback and correction log:

1. During this pass, I again attempted a `cargo test` command with multiple positional test names, which is invalid.
2. I corrected by relying on the full crate test run (`cargo test -p axiomme-core`) and confirmed the new tests are executed and passing in that run.

## FindResult Data-Model Simplification (v10)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/search.rs`
- `crates/axiomme-core/src/retrieval/engine.rs`
- `crates/axiomme-core/src/client/search/reranker.rs`
- `crates/axiomme-core/src/client/relation_service.rs`
- `crates/axiomme-core/src/client/search/result.rs`

Issue found:

1. `FindResult` carried duplicated hit payloads in four vectors (`query_results`, `memories`, `resources`, `skills`).
2. Relation enrichment and typed-edge counting walked duplicated vectors, creating repeated work and allocation pressure.
3. Reranker allocated a second hit vector by cloning all hits before score update.

Applied fix:

1. `FindResult` now has a single canonical hit list:
   - `query_results: Vec<ContextHit>`
2. Category views are explicit index buckets:
   - `hit_buckets: { memories: Vec<usize>, resources: Vec<usize>, skills: Vec<usize> }`
   - built by pure transform `classify_hit_buckets(&[ContextHit])`
3. Added explicit view accessors on `FindResult`:
   - `memories()`, `resources()`, `skills()`
4. Relation enrichment now mutates only `query_results` (single source of truth).
5. Reranker now updates scores in-place and rebuilds `hit_buckets` after sort/truncate.

Regression checks:

1. `cargo fmt --all`
2. `cargo test -p axiomme-core`
3. `cargo test -p axiomme-cli`
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. `scripts/manual_usecase_validation.sh --date 2026-02-24 --report-path docs/MANUAL_USECASE_VALIDATION_2026-02-24.md`

Self-feedback and correction log:

1. Initial compile failed because new symbols (`HitBuckets`, `classify_hit_buckets`) were not re-exported in `models/mod.rs`.
2. I fixed re-exports and reran full validation before accepting the refactor.

## Search Cutoff Enforcement Fix (v11)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/retrieval/expansion.rs`
- `crates/axiomme-core/src/retrieval/tests.rs`

Issue found:

1. `score_threshold` and `min_match_tokens` were accepted and logged in query-plan notes, but expansion-selected hits were not consistently gated by those cutoffs.
2. In practice, low-signal fallback hits could still appear because traversal propagated scores into leaf hits without enforcing the requested cutoff policy.

Applied fix:

1. Introduced explicit `QueryCutoffs` model in expansion path:
   - score gate (`score_threshold`)
   - lexical overlap gate (`min_match_tokens`)
2. Applied cutoff checks to:
   - identifier fast-path candidate list
   - global-rank baseline used by DRR
   - leaf-hit insertion during expansion traversal
3. Kept side effects isolated:
   - cutoff checks are pure transforms over `(record, score, query_tokens)`.

Regression checks:

1. Added tests:
   - `drr_enforces_min_match_tokens_for_selected_hits`
   - `drr_enforces_score_threshold_for_expansion_selected_hits`
2. Full validation passed:
   - `cargo fmt --all`
   - `cargo test -p axiomme-core`
   - `cargo test -p axiomme-cli`
   - `cargo clippy --workspace --all-targets -- -D warnings`

Real-use validation update:

1. Re-ran real dataset scenario and refreshed report:
   - `docs/REAL_CONTEXTSET_VALIDATION_2026-02-24.md`
2. CRUD delete assertion is now URI-based (deleted URI must not reappear), not hit-count-based.

Self-feedback and correction log:

1. I initially interpreted post-delete retrieval hit count as deletion failure.
2. That was a bad metric for semantic top-k retrieval. I corrected validation to assert deleted URI non-presence directly and re-ran the scenario.

## Cutoff Path Allocation Trim (v12)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/retrieval/expansion.rs`
- `crates/axiomme-core/src/index.rs`
- `crates/axiomme-core/src/index.rs` tests

Issue found:

1. `min_match_tokens` enforcement used per-record lowercase allocation across uri/name/abstract/content.
2. On large markdown corpora this adds avoidable allocation and CPU cost on the hot retrieval path.

Applied fix:

1. Added explicit index helper:
   - `InMemoryIndex::token_overlap_count(uri, query_tokens)`
   - uses existing indexed token set intersection (no content string rebuild/lowercase pass).
2. Replaced cutoff overlap checks in expansion path with index token overlap:
   - identifier fast path
   - global-rank candidate filtering
   - expansion leaf-hit insertion
3. Kept cutoff model explicit and side-effect free via `QueryCutoffs`.

Regression checks:

1. Added index test:
   - `token_overlap_count_uses_indexed_token_sets`
2. Re-ran targeted tests:
   - `drr_enforces_min_match_tokens_for_selected_hits`
   - `drr_enforces_score_threshold_for_expansion_selected_hits`
3. Full validation passed:
   - `cargo fmt --all`
   - `cargo test -p axiomme-core`
   - `cargo test -p axiomme-cli`
   - `cargo clippy --workspace --all-targets -- -D warnings`
4. Re-ran real dataset validation:
   - `docs/REAL_CONTEXTSET_VALIDATION_2026-02-24.md`

Self-feedback and correction log:

1. v11 first implementation was functionally correct but mechanically expensive due to repeated lowercase allocation.
2. I replaced it with token-set intersection over pre-indexed data and revalidated end-to-end before accepting.
3. While regenerating the real-use report, one verdict sentence incorrectly implied `search` was non-empty for all sampled headings; corrected to explicit `7/8` with cause (`min_match_tokens=2` on single-token heading).

## Search Hint/Planner Allocation Trim (v13)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/client/search/mod.rs`
- `crates/axiomme-core/src/retrieval/planner.rs`

Issue found:

1. Session OM hint path cloned full `recent_messages` even when no filtering was required.
2. Hint normalization (`normalize_hint_text`) allocated intermediate `Vec<&str>` + `join`.
3. Planner recomputed lowercased query scans in multiple places.

Applied fix:

1. Session hint path now borrows the original message slice unless OM filtering is actually required.
2. Replaced hint normalization with a single-pass whitespace-collapse + char-budget clip function.
3. Planner now computes explicit `QueryIntent` once (`wants_skill`, `wants_memory`) and reuses it for scope/aux query planning.

Regression checks:

1. Added tests:
   - `client::search::tests::normalize_hint_text_collapses_whitespace_and_clips_chars`
   - `client::search::tests::normalize_hint_text_rejects_empty_or_zero_budget`
   - `retrieval::planner::tests::query_intent_parses_skill_and_memory_flags`
2. Full validation passed:
   - `cargo fmt --all`
   - `cargo test -p axiomme-core`
   - `cargo test -p axiomme-cli`
   - `cargo clippy --workspace --all-targets -- -D warnings`
3. Real-use validation passed:
   - `scripts/manual_usecase_validation.sh --date 2026-02-24 --report-path docs/MANUAL_USECASE_VALIDATION_2026-02-24.md`

Self-feedback and correction log:

1. I initially ran `cargo test` with multiple positional filters in a single command and got CLI argument errors.
2. I corrected to one filter per invocation for targeted checks, then revalidated with full crate test runs.

## OM Hint Prefix Unification (v14)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/retrieval/planner.rs`
- `crates/axiomme-core/src/retrieval/engine.rs`

Issue found:

1. OM hint prefix checks were duplicated across planner and engine.
2. Both used `to_ascii_lowercase().starts_with("om:")`, which allocates per hint.

Applied fix:

1. Promoted planner prefix predicate to shared retrieval helper:
   - `is_om_hint(text: &str)` (case-insensitive, trim-aware, allocation-free).
2. Replaced engine query-note OM hint counting path to reuse the same helper.
3. Replaced planner non-OM hint merge from `filter + collect + join` to a single-pass builder (`merge_non_om_hints`).

Regression checks:

1. Added tests:
   - `retrieval::planner::tests::is_om_hint_is_case_insensitive_and_trim_aware`
   - `retrieval::planner::tests::merge_non_om_hints_skips_om_prefixed_entries`
2. Full validation passed:
   - `cargo fmt --all`
   - `cargo test -p axiomme-core`
   - `cargo test -p axiomme-cli`
   - `cargo clippy --workspace --all-targets -- -D warnings`
3. Real-use validation passed:
   - `scripts/manual_usecase_validation.sh --date 2026-02-24 --report-path docs/MANUAL_USECASE_VALIDATION_2026-02-24.md`

Self-feedback and correction log:

1. I again attempted targeted tests with multiple positional filters in one command and hit CLI argument errors.
2. Corrected by running one filter at a time, then full-suite validation.

## Hint Boundary + Fast-Path Allocation Trim (v15)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/client/search/mod.rs`
- `crates/axiomme-core/src/retrieval/expansion.rs`

Issue found:

1. `collapse_and_clip_whitespace` could emit trailing whitespace at strict char boundaries (e.g. `"alpha "`).
2. DRR `run_single_query` allocated `trace_id` before identifier fast-path check; on fast-path returns this UUID allocation was unused.

Applied fix:

1. Updated hint clipping boundary check to require room for both separator and at least one next token character.
2. Added regression assertion for boundary case:
   - `normalize_hint_text("alpha beta", 6) == Some("alpha")`
3. Moved `trace_id` generation in expansion flow to post fast-path branch so UUID is allocated only when needed.

Regression checks:

1. Targeted:
   - `cargo test -p axiomme-core normalize_hint_text_collapses_whitespace_and_clips_chars`
   - `cargo test -p axiomme-core normalize_hint_text_rejects_empty_or_zero_budget`
   - `cargo test -p axiomme-core identifier_query_fast_path_prefers_filename_typo_match`
2. Full:
   - `cargo fmt --all`
   - `cargo test -p axiomme-core`
   - `cargo clippy -p axiomme-core --all-targets -- -D warnings`
3. Real-use random run (contextSet):
   - dataset: `/Users/axient/Documents/contextSet`
   - sampled headings: 20 (random)
   - `find` non-empty: 20/20
   - `search` non-empty: 20/20 (`--min-match-tokens 2`)
   - expected source doc in top-5: `find` 15/20, `search` 15/20
   - misses were generic/duplicate headings (`오류 처리 및 복구`, `RULE_2_2`, `SECTION 6`, `코드 품질`) that appear across multiple files.

Self-feedback and correction log:

1. First random-run script used non-portable shell features (`mapfile`, `shuf`) and failed in zsh/macOS.
2. Then `mktemp` template and `pipefail` + `head` interaction caused additional run failures.
3. I corrected to a portable flow (full list file generation + sampled file slicing) and reran to completion before accepting results.

## Relation URI Set Invariant (v16)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/client/relation_service.rs`
- `crates/axiomme-core/src/client/tests/relation_trace_logs.rs`

Issue found:

1. `link` accepted duplicate URIs and persisted them as-is.
2. This weakens the relation model invariant ("relation endpoints are a set of unique URIs"), and could inflate relation payloads and downstream enrichment work.

Applied fix:

1. Added URI dedup normalization (`dedupe_relation_uris`) in `link`.
2. Enforced invariant: relation links now require at least two **unique** URIs.
3. Updated CRUD test to include duplicate input and assert persisted URI list is deduplicated.
4. Added explicit validation test:
   - `relation_api_requires_at_least_two_unique_uris`

Regression checks:

1. Targeted:
   - `cargo test -p axiomme-core relation_api_supports_link_unlink_and_list_crud`
   - `cargo test -p axiomme-core relation_api_requires_at_least_two_unique_uris`
2. Full:
   - `cargo fmt --all`
   - `cargo test -p axiomme-core`
   - `cargo clippy -p axiomme-core --all-targets -- -D warnings`

Self-feedback and correction log:

1. Initial review focused on retrieval quality and omitted relation write-path invariants; this was a blind spot.
2. I corrected by re-reading relation mutation paths (`link/unlink`) and tightening data-contract invariants at the core API boundary.

## Relation Boundary + Typed Edge Cache Clarification (v17)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/client/relation_service.rs`
- `crates/axiomme-core/src/client/tests/relation_trace_logs.rs`

Issue found:

1. Relation API boundary rejected `queue` writes but still allowed other internal scope (`temp`) relation operations, which made the external/internal contract less explicit.
2. Typed-edge enrichment repeatedly parsed related URIs for object-type resolution across hits, adding avoidable CPU/allocation overhead on relation-heavy result sets.

Applied fix:

1. Added explicit owner-scope gate for relation operations:
   - `relations`, `link`, `unlink` now reject internal scopes (`temp`, `queue`) at API entry with `PermissionDenied`.
2. Added typed object-type resolution cache during enrichment:
   - one request-local `HashMap<String, Option<String>>` memoizes URI -> object type resolution.
   - reused for source/target object type lookups to avoid repeated parse/resolve cycles.
3. Added regression test:
   - `relation_api_rejects_internal_temp_scope_operations`

Regression checks:

1. Targeted:
   - `cargo test -p axiomme-core relation_api_rejects_internal_temp_scope_operations`
   - `cargo test -p axiomme-core relation_enrichment_can_attach_typed_edge_metadata_when_enabled`
   - `cargo test -p axiomme-core relation_trace_logs`
2. Quality gates:
   - `cargo test -p axiomme-core`
   - `cargo clippy -p axiomme-core --all-targets -- -D warnings`

Self-feedback and correction log:

1. Initial targeted test command used `--exact` with partially qualified names, producing `0 tests` and false confidence.
2. Corrected by running name filters without `--exact`, then running broader `relation_trace_logs` and full core tests.
3. Clippy flagged a `let...else` in an `Option` path; corrected to `?` and re-ran clippy to ensure warning-free state.

## Random contextSet Benchmark Automation (v18)

Date:

- 2026-02-24

Scope:

- `scripts/contextset_random_benchmark.sh`
- `docs/ENGINEERING_PLAYBOOK_2026-02-23.md`
- `README.md`

Issue found:

1. Existing validation script (`manual_usecase_validation.sh`) is broad but deterministic; it is good for contract coverage, weak for random real-use regression detection.
2. Random benchmark evidence existed as ad-hoc one-off runs, not as repeatable automation with explicit pass/fail thresholds.

Applied fix:

1. Added reproducible random benchmark script for real dataset:
   - ingests `/Users/axient/Documents/contextSet`
   - samples markdown headings by seed
   - executes `find/search` per scenario
   - measures `non-empty`, `top1`, `top5`, and latency (`mean/p50/p95`)
   - validates CRUD (`add -> document save -> read -> rm`)
   - emits markdown + TSV report
   - returns non-zero on threshold failure.
2. Added documented invocation to engineering playbook and repository README.

Validation:

1. `scripts/contextset_random_benchmark.sh --sample-size 12 --seed 20260224 --report-path docs/REAL_CONTEXTSET_VALIDATION_2026-02-24-random.md --skip-build`
2. `scripts/contextset_random_benchmark.sh --sample-size 24 --seed 4242 --report-path docs/REAL_CONTEXTSET_VALIDATION_2026-02-24-random-seed4242.md --skip-build`

Self-feedback and correction log:

1. First implementation used `--markdown-only true` (flag misuse) and failed argument parsing.
2. Then I incorrectly used `document save` as create path for new file; fixed by using `add` for create and `document save` for update.
3. I removed an accidental placeholder loop in report rendering to keep output explicit and deterministic.
4. I found search fairness drift risk from reusing one session id across all random scenarios; corrected to per-scenario isolated session ids.

## Random Benchmark Matrix + Input Contract Hardening (v19)

Date:

- 2026-02-24

Scope:

- `scripts/contextset_random_benchmark.sh`
- `scripts/contextset_random_benchmark_matrix.sh`
- `docs/ENGINEERING_PLAYBOOK_2026-02-23.md`
- `README.md`

Issue found:

1. Single-seed random benchmark can hide seed variance; release confidence should be based on multiple independent random samples.
2. Benchmark scripts accepted loosely validated numeric inputs, making failures harder to diagnose and easier to misconfigure.
3. Matrix runner did not propagate threshold settings explicitly and did not expose per-seed failure reason/duration.
4. Candidate generation used only the first heading per markdown file, which biased retrieval coverage.

Applied fix:

1. Added path-stable defaults based on script location (`<repo>/target/...`, `<repo>/docs/...`) instead of caller CWD.
2. Added explicit numeric/range validation for sample size, seed, limits, `min-match-tokens`, and threshold percentages.
3. Extended matrix runner to pass through retrieval thresholds/options to each seed run.
4. Added per-seed runtime duration and failure-reason columns in matrix report.
5. Documented matrix benchmark command as the recommended release gate baseline.
6. Expanded random scenario candidates to all markdown headings (not first heading only) to reduce sampling bias.
7. Added explicit `top1` quality gates (`find_top1 >= 65%`, `search_top1 >= 65%`) in benchmark defaults.

Validation:

1. `scripts/contextset_random_benchmark_matrix.sh --dataset /Users/axient/Documents/contextSet --sample-size 24 --seeds 4242,777,9001 --report-path docs/REAL_CONTEXTSET_VALIDATION_MATRIX_2026-02-24.md --skip-build`
2. `cargo test -p axiomme-cli -p axiomme-core`
3. `cargo fmt --all -- --check`
4. `cargo clippy -p axiomme-cli -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Initial matrix automation existed but lacked threshold pass-through and observability (`reason`, `duration`), reducing triage quality.
2. I corrected by carrying all gate options into the per-seed benchmark call and emitting explicit per-seed diagnostics.

## Retrieval Determinism Tie-Break Hardening (v20)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/retrieval/scoring.rs`
- `crates/axiomme-core/src/retrieval/engine.rs`
- `crates/axiomme-core/src/retrieval/expansion.rs`

Issue found:

1. Retrieval merged hits from `HashMap` and sorted by score only in multiple paths.
2. For equal scores, output order could depend on hash iteration order, weakening deterministic behavior guarantees.

Applied fix:

1. Introduced a single shared hit ordering rule in `retrieval/scoring.rs`:
   - score descending,
   - URI ascending tie-break.
2. Replaced local score-only sorts in `engine` and `expansion` with the shared deterministic sorter.
3. Added regression test for equal-score deterministic ordering (`hit_sort_is_deterministic_for_equal_scores_via_uri_tiebreak`).

Validation:

1. `cargo test -p axiomme-core retrieval::scoring::tests::hit_sort_is_deterministic_for_equal_scores_via_uri_tiebreak`
2. `cargo test -p axiomme-core retrieval::tests`
3. `cargo test -p axiomme-core`
4. `cargo test -p axiomme-cli`
5. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Initial review focused on throughput and gate coverage; deterministic ordering invariants in tie cases were not explicit enough.
2. I corrected by centralizing ordering semantics in one function and wiring all retrieval merge/finalize paths to that function.

## Query Plan Keyword Noise Reduction (v21)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/retrieval/scoring.rs`

Issue found:

1. `query_plan.keywords` tokenization emitted duplicate tokens for repeated words (`oauth oauth token ...`), adding output noise without improving retrieval behavior.

Applied fix:

1. Updated keyword tokenization to:
   - normalize to lowercase,
   - preserve first-seen order,
   - deduplicate repeated tokens.
2. Added regression test (`tokenize_keywords_normalizes_and_deduplicates_in_order`).

Validation:

1. `cargo test -p axiomme-core retrieval::scoring::tests::tokenize_keywords_normalizes_and_deduplicates_in_order`
2. `cargo test -p axiomme-core retrieval::tests`
3. `cargo test -p axiomme-cli`
4. `cargo clippy -p axiomme-core -p axiomme-cli -- -D warnings`

Self-feedback and correction log:

1. The previous shape was technically correct but semantically noisy; repeated keywords made diagnostics less clear.
2. I corrected this by treating `keywords` as a set-like summary while preserving deterministic order.

## Observer Batch Parallelism Policy Hardening (v22)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/session/om/observer/threading.rs`

Issue found:

1. Batch observer parallelism used a fixed compile-time cap only.
2. Runtime operators could not tune thread fanout for constrained environments without code changes.

Applied fix:

1. Added explicit runtime policy input:
   - env: `AXIOMME_OBSERVER_BATCH_PARALLELISM`
   - hard ceiling remains `MAX_OBSERVER_BATCH_PARALLELISM` (4).
2. Default cap now resolves from available CPU parallelism, clamped to `[1, 4]`.
3. Added pure resolver function (`resolve_observer_batch_parallelism_cap`) and unit tests for:
   - default behavior,
   - valid override,
   - invalid override fallback.

Validation:

1. `cargo test -p axiomme-core observer_batch_parallelism_is_at_least_one`
2. `cargo test -p axiomme-core resolve_parallelism_cap`
3. `cargo test -p axiomme-core`
4. `cargo test -p axiomme-cli`
5. `cargo clippy -p axiomme-core -p axiomme-cli -- -D warnings`

Self-feedback and correction log:

1. Initial implementation used a manual clamp pattern and failed strict clippy.
2. I corrected it to `clamp(1, MAX_OBSERVER_BATCH_PARALLELISM)` and re-ran clippy/tests.

## Search Reranker Lock Scope Reduction (v23)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/client/search/reranker.rs`

Issue found:

1. `apply_reranker_with_mode` held the index `RwLock` read guard across full per-hit reranking computations.
2. Under concurrent indexing/write paths, this widened writer wait windows and increased tail latency risk.

Applied fix:

1. Introduced explicit `DocSignals` data model to capture only reranker-required signals per hit:
   - `doc_class`
   - `uri_or_name_overlap`
   - `tag_overlap_count`
2. Changed reranker flow to two phases:
   - phase A (lock held): collect `DocSignals` per hit URI from index records,
   - phase B (lock released): compute score boosts and reorder/truncate hits.
3. Kept scoring semantics unchanged while making lock ownership boundaries explicit.

Validation:

1. `cargo test -p axiomme-core client::search::backend_tests::doc_aware_reranker_prioritizes_config_documents`
2. `cargo test -p axiomme-core client::search::backend_tests`
3. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Initial reranker path mixed data fetch and compute under one lock scope, which was simple but hid contention cost.
2. I corrected by extracting minimal immutable signals and running pure score transforms outside the lock.

## Index Prefix Prune Allocation Reduction (v24)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`
- `crates/axiomme-core/src/client/resource_service.rs`

Issue found:

1. Prefix prune path (`rm` / `mv`) called `index.all_records()` and cloned full `IndexRecord` payloads even though only URIs were needed.
2. This inflated allocation/copy cost for large content sets and could worsen UI-visible latency on constrained devices.

Applied fix:

1. Added `InMemoryIndex::uris_with_prefix(&AxiomUri) -> Vec<String>` to collect only sorted matching URIs.
2. Replaced prune path in `prune_index_prefix_from_memory` to use URI-only collection and then remove entries.
3. Added regression test:
   - `uris_with_prefix_returns_sorted_matches_without_record_clone_requirements`

Validation:

1. `cargo test -p axiomme-core`
2. `cargo test -p axiomme-cli`
3. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Previous implementation was functionally correct but mechanically wasteful (data copy larger than required).
2. I corrected it by modeling explicit data need (URI list only) and removing unnecessary record cloning.

## Typed-Edge Enrichment Fault Isolation (v25)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/client/relation_service.rs`
- `crates/axiomme-core/src/client/tests/relation_trace_logs.rs`

Issue found:

1. Typed-edge enrichment path depended on ontology schema parse/compile success.
2. If ontology schema file became invalid, optional relation type enrichment could fail the whole read/search flow.

Applied fix:

1. Added `load_relation_ontology_schema_for_enrichment`.
2. Enrichment now degrades gracefully to untyped relation summaries when schema load fails with `OntologyViolation`.
3. Added regression test:
   - `relation_enrichment_soft_fails_when_ontology_schema_is_invalid`

Validation:

1. `cargo test -p axiomme-core client::tests::relation_trace_logs::relation_enrichment_soft_fails_when_ontology_schema_is_invalid`
2. `cargo test -p axiomme-core client::tests::relation_trace_logs`
3. `cargo test -p axiomme-core`
4. `cargo test -p axiomme-cli`
5. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. First reranker lock optimization used URI-keyed `HashMap`, which added avoidable URI clone/hash overhead.
2. Corrected reranker signal transport to `Vec<DocSignals>` aligned with hit order to keep lock-scope reduction without extra per-hit hashmap cost.

## ScoredRecord Heavy-Object Reduction (v26)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`
- `crates/axiomme-core/src/retrieval/expansion.rs`

Issue found:

1. Retrieval scoring candidate (`ScoredRecord`) held full `IndexRecord` by value.
2. `IndexRecord` includes large `content` text, so `index.search` cloned heavy payloads for many candidates before top-k cutoff.
3. This inflated allocation/copy cost in hot search path and increased memory pressure.

Applied fix:

1. `ScoredRecord` was reduced to minimal scoring/progression fields:
   - `uri`, `is_leaf`, `depth`, `exact`, `dense`, `sparse`, `recency`, `path`, `score`
2. Retrieval expansion path now resolves full records lazily only when materializing final `ContextHit`:
   - `make_hit_from_scored(index, scored)`
3. Updated frontier/global-rank logic to use lightweight fields (`uri`, `depth`, `is_leaf`) directly.

Validation:

1. `cargo test -p axiomme-core retrieval::tests::drr_returns_trace_and_hits`
2. `cargo test -p axiomme-core`
3. `cargo test -p axiomme-cli`
4. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Initial heavy-object review focused on relation/search glue layers and missed the largest hot-path clone source in index scoring output.
2. Corrected by making scored candidates data-minimal and moving record materialization to the final read boundary.

## Exact-Key Cache Memory Slimming (v27)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`

Issue found:

1. `ExactRecordKeys` cached full lowercased markdown headings and content lines as `Vec<String>`.
2. In large corpora this amplified per-document heap usage in hot index structures.
3. Exact raw-match check only needed deterministic membership lookup, not retained full line strings.

Applied fix:

1. Replaced raw-string membership caches with deterministic `u64` fingerprints:
   - `heading_lower_hashes: Vec<u64>`
   - `content_line_lower_hashes: Vec<u64>`
2. Added stable FNV-1a fingerprint helper and sorted hash membership check:
   - `stable_fingerprint64`
   - `contains_sorted_hash`
3. Extended `ExactQueryKeys` with `raw_lower_hash` so raw query membership checks stay allocation-free in scoring.
4. Kept existing scoring semantics and confidence weights unchanged.

Validation:

1. `cargo test -p axiomme-core index::tests`
2. `cargo test -p axiomme-core`
3. `cargo test -p axiomme-cli`
4. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Previous cache shape preserved more text than required for runtime decisions.
2. Corrected to keep only data needed for exact membership behavior and fuzzy/token scoring invariants.

## URI Parse Hot-Path Removal In Search (v28)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`

Issue found:

1. `InMemoryIndex::search` parsed each candidate URI into `AxiomUri` when target filtering/path scoring was enabled.
2. This repeated parse cost in tight ranking loops and added avoidable overhead under large candidate sets.

Applied fix:

1. Added boundary-safe URI prefix matcher:
   - `uri_path_prefix_match`
2. Replaced per-record parse checks with string-boundary checks for:
   - target subtree filtering
   - path score branches (`exact`, `descendant`, `ancestor`, `same-scope`)
3. Precomputed target URI text/scope root once per search invocation.

Validation:

1. `cargo test -p axiomme-core index::tests`
2. `cargo test -p axiomme-core`
3. `cargo test -p axiomme-cli`
4. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Earlier fixes reduced heavy objects but still left parse work inside the per-record scoring loop.
2. Corrected by isolating URI normalization to query-level setup and keeping candidate evaluation parse-free.

## In-Memory Index Key Deduplication (v29)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`

Issue found:

1. `InMemoryIndex` stored multiple maps keyed by `String` URI.
2. Each map owned a separate URI key allocation, creating avoidable heap duplication in large indexes.

Applied fix:

1. Switched internal map keys from `String` to `Arc<str>` for:
   - `records`
   - `vectors`
   - `token_sets`
   - `term_freqs`
   - `doc_lengths`
   - `raw_text_lower`
   - `exact_keys`
2. Upsert path now creates one shared `Arc<str>` key and clones the arc across maps (shared URI storage).
3. Kept public API unchanged (`&str` lookup remains valid through `Borrow<str>`).
4. Added boundary regression tests for parse-free target filtering:
   - `uri_path_prefix_match_respects_segment_boundaries`
   - `search_target_filter_respects_uri_boundaries_without_parse`

Validation:

1. `cargo test -p axiomme-core index::tests`
2. `cargo test -p axiomme-core`
3. `cargo test -p axiomme-cli`
4. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Initial `Arc<str>` conversion failed due accidental `&String` lookups in hot path.
2. Corrected all index lookups to `&str` and revalidated full test + clippy suite.

## Search Input Model Refactor (v30)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/client/search/mod.rs`

Issue found:

1. Search option building and request-detail logging used long positional argument lists.
2. Two `clippy::too_many_arguments` suppressions were required, reducing data-model clarity.
3. Positional argument ordering made incorrect call wiring easier under future edits.

Applied fix:

1. Added explicit input data models:
   - `SearchOptionsInput`
   - `SearchRequestLogInput`
2. Converted:
   - `build_search_options(...)` -> `build_search_options(SearchOptionsInput)`
   - `search_request_details(...)` -> `search_request_details(SearchRequestLogInput)`
3. Removed argument-count suppression attributes by making input contracts explicit.
4. Updated search tests to build options via the new input model.

Validation:

1. `cargo test -p axiomme-core client::search::tests`
2. `cargo test -p axiomme-core`
3. `cargo test -p axiomme-cli`
4. `cargo clippy -p axiomme-core -- -D warnings`

Self-feedback and correction log:

1. Earlier code kept behavior explicit but not data-first; argument lists encoded implicit structure.
2. Corrected by promoting those implicit groupings into concrete structs without changing runtime semantics.

## Large Object Scan Automation (v31)

Date:

- 2026-02-24

Scope:

- `scripts/large_object_scan.sh`

Issue found:

1. Large-struct / oversized-file review was manual and ad-hoc.
2. Repeated audits were not reproducible by command, reducing review rigor.

Applied fix:

1. Added `scripts/large_object_scan.sh` to report:
   - top Rust files by LOC
   - structs at/above configurable field threshold
2. Added configurable knobs:
   - `STRUCT_THRESHOLD` (default `12`)
   - `TOP_FILES` (default `25`)
   - `TOP_STRUCTS` (default `40`)
3. Confirmed scan highlights remaining heavy DTO candidates (`BenchmarkReport`, `ReleaseCheckDocument`) for next decomposition pass.

Validation:

1. `scripts/large_object_scan.sh .`

Self-feedback and correction log:

1. Prior audit notes identified large objects but lacked repeatable evidence generation.
2. Corrected by adding a deterministic scan script to keep future reviews measurable.

## Release Check Data Model Split + Ontology Dispatcher Simplification (v33)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/release.rs`
- `crates/axiomme-core/src/client/release/benchmark_service.rs`
- `crates/axiomme-cli/src/commands/mod.rs`

Issue found:

1. `ReleaseCheckDocument` mixed threshold/run-summary/embedding concerns in one large struct.
2. `run_validated` still carried large ontology command execution logic inline.

Applied fix:

1. Split release check contract into explicit sub-models:
   - `ReleaseCheckThresholds`
   - `ReleaseCheckRunSummary`
   - `ReleaseCheckEmbeddingMetadata`
2. `ReleaseCheckDocument` now composes those models explicitly.
3. Kept `persist_release_check_result` as a pure field mapping from `BenchmarkGateResult` into explicit release-check value objects.
4. Extracted ontology command execution path to `handle_ontology_command`.
5. Removed `clippy::too_many_lines` expectation from `run_validated` after extraction.

Validation:

1. `cargo test -p axiomme-core -- --nocapture`
2. `cargo test -p axiomme-cli -- --nocapture`
3. `cargo clippy -p axiomme-core -- -D warnings`
4. `cargo clippy -p axiomme-cli -- -D warnings`

Self-feedback and correction log:

1. Initial attempt used `#[serde(flatten)]` for sub-models to preserve flat JSON keys.
2. This triggered runtime parse failures with `u128` fields (`serde_json` limitation path).
3. Corrected by switching to explicit nested JSON objects (`thresholds`, `run_summary`, `embedding`) and re-ran full test/clippy suites.

## Add Resource Request Model + CLI Handler Split (v32)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/filesystem.rs`
- `crates/axiomme-core/src/client/resource_service.rs`
- `crates/axiomme-cli/src/commands/mod.rs`
- `crates/axiomme-cli/src/commands/handlers.rs`

Issue found:

1. Resource add API encoded request shape as positional arguments, hiding the data model.
2. CLI dispatch module kept growing; relation command logic lived in the top-level match arm.
3. Large-file scan still identified command/router files as concentration points.

Applied fix:

1. Added explicit `AddResourceRequest` model and moved add-resource call path to this contract.
2. Kept side-effect boundary unchanged (`add_resource_with_ingest_options`) but made inputs explicit and serializable.
3. Added/kept behavioral regression checks for add-resource merge semantics (`file` add does not delete sibling files).
4. Moved relation command execution into `commands/handlers.rs` (`handle_relation`) and simplified `commands/mod.rs` dispatch.

Validation:

1. `cargo test -p axiomme-core client::resource_service::tests`
2. `cargo test -p axiomme-core client::search::tests`
3. `cargo test -p axiomme-core`
4. `cargo test -p axiomme-cli`
5. `cargo clippy -p axiomme-core -- -D warnings`
6. `cargo clippy -p axiomme-cli -- -D warnings`

Self-feedback and correction log:

1. I attempted file-local `rustfmt` directly, but it failed due workspace parsing requiring Rust 2024 chain syntax support in sibling modules.
2. I did not force formatting changes beyond targeted patches; instead I validated with full compile/test/clippy to keep this iteration low-risk.

## Benchmark Gate Data Model Decomposition (v34)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/benchmark.rs`
- `crates/axiomme-core/src/client/benchmark/gate_service.rs`
- `crates/axiomme-core/src/client/release/benchmark_service.rs`
- `crates/axiomme-core/src/release_gate.rs`
- `crates/axiomme-core/src/client/tests/benchmark_suite_tests.rs`

Issue found:

1. `BenchmarkGateResult` still mixed policy, thresholds, snapshots, run execution details, and artifacts in one 22-field struct.
2. This made release-gate mapping and benchmark-gate logging depend on a wide mutable object with hidden invariants.

Applied fix:

1. Split `BenchmarkGateResult` into explicit value objects:
   - `BenchmarkGateThresholds`
   - `BenchmarkGateQuorum`
   - `BenchmarkGateSnapshot`
   - `BenchmarkGateExecution`
   - `BenchmarkGateArtifacts`
2. Updated benchmark gate construction (`empty_gate_result`, `build_gate_result`) to map into those explicit contracts.
3. Updated release check persistence and release gate decision code to read from nested contracts (`thresholds/quorum/snapshot/execution/artifacts`) with no behavioral changes.
4. Updated benchmark suite tests to assert nested data contracts explicitly.

Validation:

1. `cargo test -p axiomme-core --no-run`
2. `cargo test -p axiomme-cli --no-run`
3. `cargo test -p axiomme-core benchmark_suite_tests -- --nocapture`
4. `cargo test -p axiomme-core release_gate::tests -- --nocapture`
5. `cargo test -p axiomme-core -- --nocapture`
6. `cargo test -p axiomme-cli -- --nocapture`
7. `cargo clippy -p axiomme-core -- -D warnings`
8. `cargo clippy -p axiomme-cli -- -D warnings`

Self-feedback and correction log:

1. Initial edit broke test compile due direct field access in benchmark suite tests.
2. I switched to compile-first correction loop (`--no-run`) and updated all accesses to the nested contract shape before running full suites.
3. Re-ran large-object scan to confirm `BenchmarkGateResult` dropped out of the large-struct hotspot list.

## Benchmark Report Data Model Decomposition (v35)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/benchmark.rs`
- `crates/axiomme-core/src/client/benchmark/run_service.rs`
- `crates/axiomme-core/src/client/benchmark/gate_service.rs`
- `crates/axiomme-core/src/client/benchmark/logging_service.rs`
- `crates/axiomme-core/src/client/benchmark/report_service.rs`
- `crates/axiomme-core/src/quality.rs`
- `crates/axiomme-core/src/client/tests/benchmark_suite_tests.rs`

Issue found:

1. `BenchmarkReport` still carried 42 fields and mixed run selection, quality outcome, latency families, and artifact paths.
2. This made report construction and consumers rely on positional field knowledge instead of explicit data contracts.

Applied fix:

1. Split `BenchmarkReport` into explicit value objects:
   - `BenchmarkRunSelection`
   - `BenchmarkQualityMetrics`
   - `BenchmarkLatencySummary`
   - `BenchmarkLatencyProfile`
   - `BenchmarkArtifacts`
2. Updated benchmark run construction to map deterministic summaries into those nested models with no side-effect changes.
3. Updated gate/logging/markdown/report-writer consumers to read explicit nested paths.
4. Updated benchmark test suites and relation-trace benchmark assertion paths to the nested contract.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core --no-run`
3. `cargo test -p axiomme-core client::tests::benchmark_suite_tests -- --nocapture`
4. `cargo test -p axiomme-core client::tests::relation_trace_logs -- --nocapture`
5. `cargo test -p axiomme-core -- --nocapture`
6. `cargo test -p axiomme-cli -- --nocapture`
7. `cargo clippy -p axiomme-core -- -D warnings`
8. `cargo clippy -p axiomme-cli -- -D warnings`
9. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. Initial pass only changed models and constructors; compile errors confirmed the exact consumer surfaces that still encoded old field paths.
2. I corrected by updating `gate_service` and `quality` first (hot read paths), then tests, then ran full-suite verification.
3. Large-object scan now no longer reports `BenchmarkReport` or `BenchmarkGateResult` in the >=12 field hotspot list.

## Release Evidence Model Decomposition (v36)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/release.rs`
- `crates/axiomme-core/src/models/mod.rs`
- `crates/axiomme-core/src/client/release/reliability_service.rs`
- `crates/axiomme-core/src/client/release/evidence_service.rs`
- `crates/axiomme-core/src/release_gate.rs`
- `crates/axiomme-core/src/client/tests/release_contract_pack_tracemetrics.rs`

Issue found:

1. `ReliabilityEvidenceReport` and `OperabilityEvidenceReport` mixed execution plan, runtime outcome, probe state, and artifacts in flat DTOs.
2. Release-gate mapping and logs depended on wide field access patterns that obscured invariants and made refactor risk high.

Applied fix:

1. Decomposed `ReliabilityEvidenceReport` into explicit value objects:
   - `ReliabilityReplayPlan`
   - `ReliabilityReplayProgress`
   - `ReliabilityQueueDelta`
   - `ReliabilitySearchProbe`
2. Decomposed `OperabilityEvidenceReport` into explicit value objects:
   - `OperabilitySampleWindow`
   - `OperabilityCoverage`
3. Updated reliability/operability evidence services and release-gate decision mapping to consume nested contracts.
4. Updated end-to-end release evidence tests to assert nested contract fields.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core client::tests::release_contract_pack_tracemetrics -- --nocapture`
3. `cargo test -p axiomme-core -- --nocapture`
4. `cargo test -p axiomme-cli -- --nocapture`
5. `cargo clippy -p axiomme-core -- -D warnings`
6. `cargo clippy -p axiomme-cli -- -D warnings`
7. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. Initial documentation patch duplicated one struct entry in large-object summary.
2. Corrected immediately and re-checked the generated hotspot list against the latest scan output.
3. Scan now confirms both `ReliabilityEvidenceReport` and `OperabilityEvidenceReport` are removed from the >=12-field hotspot list.

## Retrieval Finalize Input Decomposition (v37)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/retrieval/expansion.rs`

Issue found:

1. `FinalizeSingleQueryInput` mixed query context, candidate ranking state, and trace runtime metrics in one wide input contract.
2. This made the finalize step harder to reason about and increased accidental coupling risk.

Applied fix:

1. Replaced `FinalizeSingleQueryInput` with explicit grouped inputs:
   - `FinalizeRunContext`
   - `FinalizeRunCandidates`
   - `FinalizeRunTrace`
2. Kept finalize behavior unchanged (global leaf baseline merge, deterministic sorting, trace emission).
3. Updated call sites to pass explicit grouped data, making effect boundaries visible.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core retrieval::tests -- --nocapture`
3. `cargo test -p axiomme-core -- --nocapture`
4. `cargo test -p axiomme-cli -- --nocapture`
5. `cargo clippy -p axiomme-core -- -D warnings`
6. `cargo clippy -p axiomme-cli -- -D warnings`
7. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. First pass focused on type split only; then I re-ran retrieval-specific tests first to prove hot-path invariants before full suite.
2. Large-object scan confirmed `FinalizeSingleQueryInput` no longer appears in hotspot output.

## Ontology Probe Data-Model Decomposition (v38)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/release.rs`
- `crates/axiomme-core/src/models/mod.rs`
- `crates/axiomme-core/src/release_gate.rs`

Issue found:

1. `OntologyContractProbeResult` combined probe command status, schema version checks, schema size counts, and invariant summary in one 12-field object.
2. The mixed shape blurred domain boundaries inside release contract integrity flow.

Applied fix:

1. Split probe payload into explicit nested value objects:
   - `OntologySchemaVersionProbe`
   - `OntologySchemaCardinality`
   - `OntologyInvariantCheckSummary`
2. Updated release-gate probe builders (`from_error`, `run_ontology_contract_probe`) to emit nested contracts directly.
3. Kept gate pass/fail semantics unchanged.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core release_gate::tests -- --nocapture`
3. `cargo test -p axiomme-core client::tests::release_contract_pack_tracemetrics -- --nocapture`
4. `cargo test -p axiomme-core -- --nocapture`
5. `cargo test -p axiomme-cli -- --nocapture`
6. `cargo clippy -p axiomme-core -- -D warnings`
7. `cargo clippy -p axiomme-cli -- -D warnings`
8. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. I validated release-gate targeted tests before full suite to minimize debugging surface if contract assertions failed.
2. Re-ran full scan and confirmed `OntologyContractProbeResult` dropped from >=12-field hotspot list.

## Release Pack Options Decomposition (v39)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/benchmark.rs`
- `crates/axiomme-core/src/models/mod.rs`
- `crates/axiomme-core/src/client/release/pack_service.rs`
- `crates/axiomme-cli/src/commands/handlers.rs`
- `crates/axiomme-core/src/client/tests/release_contract_pack_tracemetrics.rs`

Issue found:

1. `ReleaseGatePackOptions` carried reliability/eval/benchmark/operability/security parameters in one flat 18-field shape.
2. The flat shape hid execution intent and made call-sites depend on many positional field names.

Applied fix:

1. Split release pack options into explicit plan contracts:
   - `ReleaseGateReplayPlan`
   - `ReleaseGateOperabilityPlan`
   - `ReleaseGateEvalPlan`
   - `ReleaseGateBenchmarkRunPlan`
   - `ReleaseGateBenchmarkGatePlan`
2. Updated `ReleaseGatePackOptions` to compose those plans plus workspace/security mode.
3. Updated CLI handler mapping and release pack orchestration service to consume nested plans.
4. Updated release pack integration test options fixture to the new grouped shape.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo check -p axiomme-cli`
3. `cargo test -p axiomme-core client::tests::release_contract_pack_tracemetrics -- --nocapture`
4. `cargo test -p axiomme-cli cli::tests::release_pack -- --nocapture`
5. `cargo test -p axiomme-core -- --nocapture`
6. `cargo test -p axiomme-cli -- --nocapture`
7. `cargo clippy -p axiomme-core -- -D warnings`
8. `cargo clippy -p axiomme-cli -- -D warnings`
9. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. I first validated targeted release-pack tests before full-suite runs to ensure mapping changes were correct.
2. Re-ran full scan and confirmed `ReleaseGatePackOptions` was removed from >=12-field hotspot list.

## Observer Config Decomposition (v40)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/session/om.rs`
- `crates/axiomme-core/src/session/om/tests.rs`
- `crates/axiomme-core/src/session/om/observer/llm.rs`
- `crates/axiomme-core/src/session/om/observer/response.rs`
- `crates/axiomme-core/src/session/om/observer/threading.rs`

Issue found:

1. `OmObserverConfig` mixed observer mode flags, LLM transport/model knobs, and text budgets in a single 13-field object.
2. Runtime call-sites had implicit coupling between model behavior and truncation limits.

Applied fix:

1. Split config into explicit nested value objects:
   - `OmObserverLlmConfig`
   - `OmObserverTextBudget`
2. Kept `OmObserverConfig` as orchestration root with:
   - `mode`
   - `model_enabled`
   - `llm`
   - `text_budget`
3. Updated observer modules and tests to access explicit nested fields.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core session::om:: -- --nocapture`
3. `cargo test -p axiomme-core -- --nocapture`
4. `cargo test -p axiomme-cli -- --nocapture`
5. `cargo clippy -p axiomme-core -- -D warnings`
6. `cargo clippy -p axiomme-cli -- -D warnings`
7. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. Initial pass refactored root struct only; second pass updated all observer module field accesses to remove mixed naming.
2. Re-ran targeted OM tests before full-suite to isolate behavioral regressions in observer logic.

## Benchmark Amortized Report Decomposition (v41)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/benchmark.rs`
- `crates/axiomme-core/src/models/mod.rs`
- `crates/axiomme-core/src/client/benchmark/run_service.rs`
- `crates/axiomme-core/src/client/tests/benchmark_suite_tests.rs`

Issue found:

1. `BenchmarkAmortizedReport` mixed run-selection inputs, wall/p95 timing, and averaged quality outcomes in one 18-field object.
2. The flat shape obscured invariants between selection/timing/quality summaries.

Applied fix:

1. Split amortized report into grouped summaries:
   - `BenchmarkAmortizedSelection`
   - `BenchmarkAmortizedTiming`
   - `BenchmarkAmortizedQualitySummary`
2. Updated root `BenchmarkAmortizedReport` to compose:
   - `mode`, `iterations`, `selection`, `timing`, `quality`, `runs`
3. Updated amortized benchmark test assertions to use nested summary paths.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core client::tests::benchmark_suite_tests::benchmark_amortized_mode_runs_multiple_iterations_in_process -- --nocapture`
3. `cargo test -p axiomme-core -- --nocapture`
4. `cargo test -p axiomme-cli -- --nocapture`
5. `cargo clippy -p axiomme-core -- -D warnings`
6. `cargo clippy -p axiomme-cli -- -D warnings`
7. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. I intentionally split only the amortized report first because it has low usage fan-out and low contract risk.
2. After targeted pass, I revalidated full-suite before moving to the final remaining hotspot.

## Eval Loop Report Decomposition (v42)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/models/eval.rs`
- `crates/axiomme-core/src/models/mod.rs`
- `crates/axiomme-core/src/client/eval/report_service.rs`
- `crates/axiomme-core/src/client/eval/logging_service.rs`
- `crates/axiomme-core/src/quality.rs`
- `crates/axiomme-core/src/release_gate.rs`
- `crates/axiomme-core/src/client/tests/eval_suite_tests.rs`
- `crates/axiomme-core/src/client/tests/relation_trace_logs.rs`

Issue found:

1. `EvalLoopReport` combined run selection, corpus coverage, quality buckets/failures, and artifact URIs in one 19-field object.
2. Release-gate and quality formatting consumed this flat shape directly, increasing accidental coupling.

Applied fix:

1. Split eval report into explicit grouped contracts:
   - `EvalRunSelection`
   - `EvalCoverageSummary`
   - `EvalQualitySummary`
   - `EvalArtifacts`
2. Updated `EvalLoopReport` to compose those grouped contracts.
3. Updated report writer, logging, markdown formatter, release gate evaluation, and tests to use nested paths.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core -- --nocapture`
3. `cargo test -p axiomme-cli -- --nocapture`
4. `cargo clippy -p axiomme-core -- -D warnings`
5. `cargo clippy -p axiomme-cli -- -D warnings`
6. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. First compile surfaced only the primary consumer errors; I fixed runtime call-sites first, then test fixtures, then re-ran full-suite.
2. Final scan confirmed no production structs remain above the configured large-object threshold (`12` fields).

## Filter Projection Allocation Trim (v43)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`
- `crates/axiomme-core/src/retrieval/expansion.rs`

Issue found:

1. `filter_projection_uris` built `HashSet<String>` on each query path, cloning URI strings for leaf matches and ancestor chain expansion.
2. DRR expansion and in-memory search consumed that set in hot paths, so repeated query execution incurred avoidable allocation churn.

Applied fix:

1. Changed projection set representation from `HashSet<String>` to `HashSet<Arc<str>>`.
2. Reused canonical URI keys from `self.records` via `get_key_value` for parent traversal, so projection membership uses shared arc keys.
3. Updated retrieval expansion filter boundary types to consume `HashSet<Arc<str>>` explicitly.
4. Kept fallback behavior for missing parent records by inserting `Arc::from(uri)` once and stopping traversal.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core retrieval::tests::drr_applies_filter_in_child_and_fallback_paths -- --nocapture`
3. `cargo test -p axiomme-core index::tests::tag_filter_limits_leaf_results -- --nocapture`
4. `cargo test -p axiomme-core -- --nocapture`
5. `cargo test -p axiomme-cli -- --nocapture`
6. `cargo clippy -p axiomme-core -- -D warnings`
7. `cargo clippy -p axiomme-cli -- -D warnings`
8. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. Initial compile failed because one call-site passed `&String` into `HashSet<Arc<str>>::contains`; corrected to `record.uri.as_str()`.
2. Re-ran targeted filter tests first, then full regression suite to ensure no semantic drift in tag/mime and DRR filter behavior.

## Expansion Child Clone Trim (v44)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`
- `crates/axiomme-core/src/retrieval/expansion.rs`
- `docs/LARGE_OBJECT_SCAN_2026-02-24.md`

Issue found:

1. `InMemoryIndex::children_of` cloned full `IndexRecord` values for each expansion step.
2. `IndexRecord` includes large payload fields (`abstract_text`, `content`, `tags`), so traversal incurred avoidable allocation and copy cost in retrieval hot paths.

Applied fix:

1. Added explicit traversal DTO `IndexChildRecord { uri, is_leaf, depth }` and changed `children_of` to return that compact structure.
2. Expansion loop now traverses using compact child records and resolves full record only when a leaf is accepted into selected hits.
3. Added unit test `children_of_returns_sorted_child_records` to lock deterministic traversal ordering/shape.
4. Refreshed large-object report file to reflect current `index.rs` LOC.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core -- --nocapture`
3. `cargo test -p axiomme-cli -- --nocapture`
4. `cargo clippy -p axiomme-core -- -D warnings`
5. `cargo clippy -p axiomme-cli -- -D warnings`
6. `bash scripts/large_object_scan.sh`
7. `cargo test -p axiomme-core index::tests::children_of_returns_sorted_child_records -- --nocapture`
8. `cargo test -p axiomme-core retrieval::tests::drr_applies_filter_in_child_and_fallback_paths -- --nocapture`

Self-feedback and correction log:

1. First patch broke build because `make_hit` still required `IndexRecord`; fixed by resolving full record only at leaf-hit acceptance.
2. Clippy flagged a nested `if`; collapsed condition using `&& let` form.
3. I attempted running two test names in one `cargo test` command and corrected to separate commands.

## Parent-Child Adjacency Index (v45)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`
- `docs/LARGE_OBJECT_SCAN_2026-02-24.md`

Issue found:

1. `children_of(parent_uri)` still scanned all records and filtered by `parent_uri`, which made each expansion step `O(N)`.
2. On large corpora this becomes dominant traversal overhead even after eliminating full `IndexRecord` clones.

Applied fix:

1. Added explicit adjacency index to `InMemoryIndex`:
   - `children_by_parent: HashMap<Arc<str>, BTreeMap<Arc<str>, ChildIndexEntry>>`
2. `children_of` now reads directly from adjacency map (`O(k)` for children count `k`) with deterministic lexical order from `BTreeMap`.
3. Wired side effects explicitly into mutating paths:
   - `upsert`: remove previous parent edge (if existing) then insert current edge
   - `remove`: remove parent edge before lexical/vector cleanup
   - `clear`: clears adjacency map
4. Added regression test for reparent+remove consistency:
   - `children_of_tracks_reparent_and_remove_consistently`

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core index::tests::children_of_returns_sorted_child_records -- --nocapture`
3. `cargo test -p axiomme-core index::tests::children_of_tracks_reparent_and_remove_consistently -- --nocapture`
4. `cargo test -p axiomme-core -- --nocapture`
5. `cargo test -p axiomme-cli -- --nocapture`
6. `cargo clippy -p axiomme-core -- -D warnings`
7. `cargo clippy -p axiomme-cli -- -D warnings`
8. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. I considered storing child metadata inside `records`-only lookup to avoid extra index state, but that retains `O(N)` scan; rejected as not mechanically sympathetic.
2. I initially used parallel verification commands, then confirmed completion explicitly per session to avoid reporting partial results.

## Ancestor Filter Traversal Rewrite (v46)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`
- `docs/LARGE_OBJECT_SCAN_2026-02-24.md`

Issue found:

1. `has_matching_leaf_descendant` still used URI prefix scan over all records, so ancestor filter checks remained `O(N)` even after adjacency index introduction.
2. Prefix-based fallback mixed two ancestry models (`uri path` vs `parent_uri graph`), leaving invariants implicit.

Applied fix:

1. Rewrote `has_matching_leaf_descendant` to traverse `children_by_parent` graph only (stack + visited set).
2. Declared invariant in code: parent-child graph is source of truth for ancestry checks.
3. Added regression test:
   - `record_matches_filter_uses_parent_chain_not_uri_prefix`
   - verifies ancestor filter follows explicit `parent_uri` chain, not URI-string prefix coincidence.

Validation:

1. `cargo check -p axiomme-core`
2. `cargo test -p axiomme-core index::tests::record_matches_filter_uses_parent_chain_not_uri_prefix -- --nocapture`
3. `cargo test -p axiomme-core retrieval::tests::drr_applies_filter_in_child_and_fallback_paths -- --nocapture`
4. `cargo test -p axiomme-core -- --nocapture`
5. `cargo test -p axiomme-cli -- --nocapture`
6. `cargo clippy -p axiomme-core -- -D warnings`
7. `cargo clippy -p axiomme-cli -- -D warnings`
8. `bash scripts/large_object_scan.sh`

Self-feedback and correction log:

1. Initial adjacency work left this path behind; I corrected that gap in this follow-up pass.
2. I evaluated dropping `visited` for less allocation, but retained it to make cycle-safety explicit and deterministic under malformed graph input.

## Heading Exact-Match Stability Hardening (v47)

Date:

- 2026-02-24

Scope:

- `crates/axiomme-core/src/index.rs`
- `crates/axiomme-core/src/client/indexing_service.rs`
- `crates/axiomme-core/src/client/indexing_service/helpers.rs`
- `scripts/contextset_random_benchmark.sh`

Issue found:

1. Real-use random benchmark had seed-sensitive top1 failures (`sample-size=12`) even when non-empty/top5 rates were stable.
2. Root causes were mixed:
   - exact-key extraction treated fenced code comment lines (`# ...` inside code block) as markdown headings
   - heading/line key windows were biased toward the beginning of large documents
   - benchmark heading candidate extraction also sampled fenced code comment lines as query headings

Applied fix:

1. Hardened heading extraction in index exact-key path:
   - `markdown_heading_lowers` now ignores fenced blocks (``` / ~~~)
   - heading key collection now uses explicit head+tail unique window (not begin-only bias)
2. Hardened content-line exact-key collection:
   - `normalized_content_line_lowers` now uses explicit head+tail unique window
3. For truncated markdown indexing:
   - appended bounded tail heading-key appendix from full-file scan (`MAX_TRUNCATED_MARKDOWN_TAIL_HEADING_KEYS`)
   - tail heading scan ignores fenced blocks
4. Hardened benchmark query-candidate extraction:
   - excludes YAML front matter and fenced blocks before selecting markdown headings

Validation:

1. Targeted tests:
   - `index::tests::markdown_heading_lowers_ignores_fenced_code_comments`
   - `index::tests::markdown_heading_lowers_keeps_tail_window_under_limit`
   - `index::tests::normalized_content_line_lowers_keeps_head_and_tail_under_limit`
   - `index::tests::deep_markdown_heading_signal_uses_tail_window_for_exact_match`
   - `client::indexing_service::tests::reindex_uri_tree_truncated_markdown_appends_tail_heading_keys`
2. Full quality gates:
   - `cargo test -p axiomme-core -- --nocapture`
   - `cargo test -p axiomme-cli -- --nocapture`
   - `cargo clippy -p axiomme-core -- -D warnings`
   - `cargo clippy -p axiomme-cli -- -D warnings`
3. Real dataset matrix benchmarks:
   - `docs/REAL_CONTEXTSET_VALIDATION_MATRIX_2026-02-24-recheck12-after-fencefix.md` => `PASS` (`3/3` seeds)
   - `docs/REAL_CONTEXTSET_VALIDATION_MATRIX_2026-02-24-recheck24-after-fencefix.md` => `PASS` (`3/3` seeds)
4. Spot query replay (real dataset ingest):
   - `find "코드 리뷰 최적화"` top1 => `axiom://resources/contextSet/contexts/action/git-integration.md` (score `1.2330983`)
   - `find "SECTION 5: 서비스 레이어"` top1 => `axiom://resources/contextSet/tools/frameworks/actix-web.md` (score `1.2425932`)
   - `find "개발 서버 시작"` top1 => `axiom://resources/contextSet/combinations/development/web-svelte-typescript.md` (score `1.1500907`)

Self-feedback and correction log:

1. Initial fix over-focused on truncated markdown tail headings; it did not change live ranking for representative failures because many misses came from fenced code handling and key-window bias.
2. I corrected by moving to contract-level parsing fixes (fence-aware heading extraction + head/tail key windows) and by aligning benchmark input semantics with the same parser boundary.
