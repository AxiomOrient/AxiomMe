# Embedding and Vector Options

Date: 2026-02-10

## 1. Constraints

- Current direction: no external API key dependency.
- Goal: maximize retrieval quality while minimizing embedding cost.
- Scope: local/offline-first execution.

## 2. Current Code Baseline

- Local retrieval already has lexical ranking with BM25-style scoring and overlap/literal boosts.
- Dense signal exists but is currently lightweight hash/semantic-lite.
- Qdrant integration currently uses dense vectors for upsert/search.

## 3. Embedding Options (ranked by current fit)

1. No embedding (lexical-only)
- Method: BM25/full-text and metadata filtering only.
- Pros: zero embedding cost, deterministic, no model dependency.
- Cons: weaker semantic recall for synonym/paraphrase/multilingual queries.

2. Sparse embedding (local)
- Method: local sparse model pipeline (e.g., BM25/miniCOIL/SPLADE style sparse vectors).
- Pros: strong lexical-semantic bridge, lower cost than dense transformer in many cases.
- Cons: additional indexing pipeline and sparse-query plumbing required.

3. Dense embedding (local)
- Method: local ONNX/gguf embedding model.
- Pros: best semantic similarity for paraphrase intent.
- Cons: highest indexing/query cost, model/version lifecycle burden.

4. External embedding API
- Method: hosted embedding provider.
- Pros: best model quality with low local ops burden.
- Cons: API key + network dependency (currently out of policy).

## 4. Vector Storage Options

1. In-memory index only
- Fit: simplest local workflow, low operational overhead.

2. Qdrant dense vectors (HNSW)
- Fit: fast ANN on dense vectors, good for semantic retrieval.

3. Qdrant sparse vectors / full-text path
- Fit: lexical-first and sparse-first retrieval with lower embedding dependence.

4. Qdrant hybrid dense + sparse
- Fit: highest retrieval quality ceiling, highest complexity.

## 5. Decision for Current Phase

- Primary runtime profile: lexical-first (BM25-centric) with embedding minimization.
- Local semantic embedder (`semantic-lite`) is the default vector profile.
- Deterministic hash embedder remains available as explicit fallback via `AXIOMME_EMBEDDER=hash`.
- Defer API-key-based providers.

## 6. Immediate Execution Plan

1. Keep quality gates strict (`fmt`, `clippy -D warnings`, `test`, prohibited-token scan).
2. Continue non-embedding critical work (replacement-equivalence verification).
3. Revisit embedding provider only after local sparse/dense tradeoff benchmark is in place.

## 7. Operational Policy (Implemented)

- Persist an index profile stamp (`stack + embedder provider/version + qdrant target`) in SQLite.
- Enforce full reindex when the stored stamp differs from current runtime profile.
- Store vector metadata (`vector_provider`, `vector_version`, `vector_dim`) in Qdrant payload for observability.
