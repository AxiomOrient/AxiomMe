# Retrieval Stack

이 문서는 검색 경로의 source of truth 를 짧게 고정합니다.

## Canonical Shape
- query entrypoint: `find`, `search`, `search_with_request`
- runtime retrieval backend policy: `memory_only`
- runtime ranking and top-k: memory index
- persisted source of truth: SQLite projection

## SQLite Role
- `context.db` 는 queue, checkpoint, OM state, persisted search projection 을 함께 저장한다.
- `search_docs` 는 검색 문서의 canonical persisted projection 이다.
- `search_doc_tags` 는 tag projection 이다.
- `search_docs_fts` 는 FTS5 lexical acceleration layer 다.
- FTS5 는 SQLite 내부 prototype 이고, runtime backend 를 SQLite 로 바꾸는 의미는 아니다.

## FTS Bootstrap Safety
- FTS bootstrap completeness 는 `system_kv.search_docs_fts_schema_version` marker 로 추적한다.
- marker 가 없거나 schema version 이 다르면 migration 이 `search_docs_fts` rebuild 를 다시 수행한다.
- marker 는 rebuild 성공 후에만 갱신된다.
- 따라서 virtual table 이 이미 있어도 interrupted migration 뒤에는 안전하게 backfill 이 재시도된다.

## Query Path
1. ingest/replay 가 `search_docs` projection 을 갱신한다.
2. FTS5 trigger 가 `search_docs_fts` 를 동기화한다.
3. startup 에서 memory index 를 복원한다.
4. query 시 runtime 이 memory index 에서 선택한다.

## Compatibility Surface
- `FindResult.query_results` 가 canonical ordered hit list 다.
- `FindResult.hit_buckets` 가 hit category 의 canonical index map 이다.
- `FindResult.memories`, `resources`, `skills` 는 compatibility view 다.
- compatibility view 는 독립 source of truth 가 아니라 `query_results + hit_buckets` 에서 파생된다.

## Legacy Boundary
- 런타임은 legacy DB 파일명 탐색이나 별도 저장소 cutover 를 지원하지 않는다.
- 대신 현재 `context.db` 안의 known legacy schema 흔적은 migration 단계에서 좁게 정리할 수 있다.
- 현재 허용된 legacy cleanup 은 `search_docs_fts` plain table 을 virtual table 로 교체하는 repair 와 bootstrap marker 기준 rebuild 재시도다.

## Evidence
- FTS projection sync: `cargo test -q -p axiomsync state::tests::search_documents_fts_tracks_upsert_and_remove`
- interrupted bootstrap recovery: `cargo test -q -p axiomsync state::tests::migration_rebuilds_fts_when_bootstrap_marker_is_missing`
- runtime lexical comparison: `cargo test -q -p axiomsync client::tests::core_editor_retrieval::fts5_prototype_matches_runtime_top_hit_for_exact_lexical_query`
