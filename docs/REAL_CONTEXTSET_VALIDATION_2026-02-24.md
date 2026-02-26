# Real Dataset Validation (contextSet)

Date: 2026-02-24
Root: `/var/folders/4z/9ns2598n15s5v2t8g3htr5x00000gn/T/axiomme-real-root.IfB41P8ZFg`
Dataset: `/Users/axient/Documents/contextSet`

## Ingest
- Status: ok
- Recursive entries listed: 113
- Tree root: axiom://resources/contextSet

## Heading Retrieval (Random 8 markdown headings)
- Total sampled headings: 8
- `find` non-empty hits: 8 / 8
- `search` non-empty hits: 7 / 8

| file | heading | find_hits | search_hits |
|---|---|---:|---:|
| `/Users/axient/Documents/contextSet/layers/task/code-implementation.md` | 단위 테스트 | 5 | 5 |
| `/Users/axient/Documents/contextSet/tools/platforms/web-platform.md` | SECTION 1: 웹 표준 및 브라우저 호환성 | 5 | 5 |
| `/Users/axient/Documents/contextSet/README.md` | docs | 5 | 0 |
| `/Users/axient/Documents/contextSet/combinations/development/ai-python-pytorch.md` | Weights & Biases 동기화 | 5 | 1 |
| `/Users/axient/Documents/contextSet/tools/platforms/backend-platform.md` | Python 리소스 관리 | 5 | 5 |
| `/Users/axient/Documents/contextSet/SYSTEM_OVERVIEW.md` | 1. Context Resolution: Load referenced contexts (REQ-2.1, DES-3.2, AC-1) | 5 | 5 |
| `/Users/axient/Documents/contextSet/expertise/analyst/system-analysis.md` | 오류 처리 및 복구 | 5 | 5 |
| `/Users/axient/Documents/contextSet/combinations/development/ai-python-pytorch.md` | 스택 개요 | 5 | 5 |

## CRUD Validation (markdown)
- Update uri: `axiom://resources/contextSet/manual-crud/axiomme-crud-md.kCvRuVEmPI.md`
- Update read-back check: pass (contains `update-pass-2026`)
- Delete check: pass (deleted URI not present in post-delete retrieval)
- Post-delete retrieval hit count (semantic top-k): 10

## Verdict
- Runtime ingest/find/search/read/document-save/rm executed successfully on real dataset.
- Heading-based retrieval remained stable (`find`: 8/8, `search`: 7/8). One `search` miss was a single-token heading (`docs`) under `min_match_tokens=2`.
- CRUD semantics validated with URI-level deletion assertion.
