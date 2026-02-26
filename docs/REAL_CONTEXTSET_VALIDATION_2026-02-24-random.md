# Real Dataset Random Benchmark (contextSet)

Date: 2026-02-24
Seed: 4242
Root: `/tmp/axiomme-contextset-root-mrQ7Kl`
Dataset: `/Users/axient/Documents/contextSet`
Target URI: `axiom://resources/contextSet`
Raw data: `/Users/axient/repository/AxiomMe/docs/REAL_CONTEXTSET_VALIDATION_2026-02-24-random.tsv`

## Ingest
- Status: ok
- Recursive entries listed: 107
- Tree root: axiom://resources/contextSet

## Random Retrieval Metrics
- Sampled heading scenarios: 10 (candidate headings: 1622)
- Unique headings in sample: 10 (ambiguous duplicates: 0)
- search min-match filter applied scenarios: 10/10 (min-match-tokens=2)
- find non-empty: 10/10 (100.00%)
- search non-empty: 10/10 (100.00%)
- find top1 expected-uri: 6/10 (60.00%)
- search top1 expected-uri: 6/10 (60.00%)
- find top5 expected-uri: 10/10 (100.00%)
- search top5 expected-uri: 10/10 (100.00%)

## Latency (ms)
- find mean/p50/p95: 7.10 / 7 / 8
- search mean/p50/p95: 7.30 / 7 / 9

## CRUD Validation
- Create uri: `axiom://resources/contextSet/manual-crud/auto-crud-4242.md`
- Create status: ok
- Update status: ok
- Read-back contains update token: pass (`crud-update-4242`)
- Delete check: pass (not readable, not listed)

## Thresholds
- min find non-empty rate: 90%
- min search non-empty rate: 80%
- min find top5 rate: 50%
- min search top5 rate: 45%

## Sample Rows

| file | heading | find_hits | search_hits | find_top1 | find_top5 | search_top1 | search_top5 | find_latency_ms | search_latency_ms |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `/Users/axient/Documents/contextSet/contexts/action/refactoring.md` | 안전성 보장 | 5 | 4 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/tools/languages/rust-cargo-workspace.md` | 의존성 트리 확인 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/tools/languages/swift.md` | SECTION 4: 에러 처리 및 안전성 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/tools/languages/python.md` | 3. E2E 테스트 (5-10%): 전체 시스템 워크플로우 | 5 | 5 | 1 | 1 | 1 | 1 | 8 | 9 |
| `/Users/axient/Documents/contextSet/tools/languages/typescript.md` | RULE_4_1: 구조화된 에러 처리 | 5 | 5 | 0 | 1 | 0 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/combinations/development/web-svelte-typescript.md` | 개발 서버 시작 | 5 | 5 | 0 | 1 | 0 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/combinations/development/web-svelte-typescript.md` | 2. 커스텀 액션 (Actions) | 5 | 5 | 1 | 1 | 1 | 1 | 8 | 8 |
| `/Users/axient/Documents/contextSet/contexts/action/refactoring.md` | 도구 및 통합 | 5 | 5 | 0 | 1 | 0 | 1 | 6 | 7 |
| `/Users/axient/Documents/contextSet/combinations/development/mobile-ios-swiftui.md` | RESOLUTION_LEVEL: OVERVIEW | 5 | 5 | 0 | 1 | 0 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/tools/languages/python.md` | 제너레이터를 사용한 메모리 효율적인 처리 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |

## Verdict
- Status: PASS
- Reasons: none
