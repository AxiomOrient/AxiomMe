# SQLite FTS5 Prototype

`T20`은 retrieval backend 를 SQLite 로 바꾸는 작업이 아니라, `search_docs` canonical projection 위에 FTS5 lexical acceleration layer 를 실제로 붙여보는 prototype 입니다.

## Prototype Shape
- `search_docs` 는 canonical projection 그대로 유지
- `search_docs_fts` 는 FTS5 virtual table
- `search_docs` insert/update/delete 와 함께 trigger 로 동기화
- migration 시 `rebuild` 로 기존 projection 을 다시 채움
- bootstrap completeness 는 `system_kv.search_docs_fts_schema_version` marker 로 추적
- marker 가 없거나 version 이 다르면 rebuild 를 다시 수행

## Evidence

```bash
cargo test -q -p axiomsync state::tests::search_documents_fts_tracks_upsert_and_remove
cargo test -q -p axiomsync state::tests::migration_rebuilds_fts_when_bootstrap_marker_is_missing
cargo test -q -p axiomsync client::tests::core_editor_retrieval::fts5_prototype_matches_runtime_top_hit_for_exact_lexical_query
```

이 세 테스트는 아래를 고정한다.
- FTS5 projection 이 `upsert/remove`와 같이 움직이는지
- interrupted bootstrap 뒤에도 marker 기준으로 rebuild 가 재시도되는지
- lexical exact query 에서 FTS5 top hit 와 runtime `find` top hit 가 같은지

## Why This Closes T20
- prototype schema 가 migration 에 들어갔다.
- rebuild flow 와 bootstrap marker 가 migration 에 들어갔다.
- retrieval comparison evidence 가 테스트로 남았다.
- retrieval backend policy 는 여전히 `memory_only`라서 canonical runtime contract 는 깨지지 않는다.
