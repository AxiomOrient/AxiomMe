# Changelog

## v1.2.0 - 2026-03-14

- clarified runtime and documentation boundaries around `context.db`, `memory_only` retrieval, and ownership routing
- added runtime baseline tooling for cold boot, warm boot, reindex, search, and queue replay measurement
- improved SQLite hot paths with busy timeout, ordered restore index, and outbox due-time indexing
- added SQLite FTS5 prototype over `search_docs` with trigger sync and lexical comparison coverage
- made FTS bootstrap rebuild crash-safe with `system_kv` schema marker retry
- removed duplicate benchmark retrieval work so trace latency reporting reuses one retrieval measurement
