# Real Dataset Random Benchmark (contextSet)

Date: 2026-02-24
Seed: 4242
Root: `/tmp/axiomme-contextset-root-23hFJO`
Dataset: `/Users/axient/Documents/contextSet`
Target URI: `axiom://resources/contextSet`
Raw data: `docs/REAL_CONTEXTSET_VALIDATION_2026-02-24-random-sanity8.tsv`

## Ingest
- Status: ok
- Recursive entries listed: 107
- Tree root: axiom://resources/contextSet

## Random Retrieval Metrics
- Sampled heading scenarios: 8 (candidate headings: 1268)
- Unique headings in sample: 8 (ambiguous duplicates: 0)
- search min-match filter applied scenarios: 8/8 (min-match-tokens=2)
- find non-empty: 8/8 (100.00%)
- search non-empty: 8/8 (100.00%)
- find top1 expected-uri: 7/8 (87.50%)
- search top1 expected-uri: 7/8 (87.50%)
- find top5 expected-uri: 8/8 (100.00%)
- search top5 expected-uri: 8/8 (100.00%)

## Latency (ms)
- find mean/p50/p95: 5.75 / 6 / 7
- search mean/p50/p95: 5.62 / 6 / 7

## CRUD Validation
- Create uri: `axiom://resources/contextSet/manual-crud/auto-crud-4242.md`
- Create status: ok
- Update status: ok
- Read-back contains update token: pass (`crud-update-4242`)
- Delete check: pass (not readable, not listed)

## Thresholds
- min find non-empty rate: 90%
- min search non-empty rate: 80%
- min find top1 rate: 65%
- min search top1 rate: 65%
- min find top5 rate: 50%
- min search top5 rate: 45%

## Sample Rows

| file | heading | find_hits | search_hits | find_top1 | find_top5 | search_top1 | search_top5 | find_latency_ms | search_latency_ms |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `/Users/axient/Documents/contextSet/layers/system/orchestration.md` | 병렬 처리 전략 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/tools/platforms/backend-platform.md` | SECTION 5: 모니터링 및 관찰성 요구사항 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/design.md` | 4. 지식 관리 시스템 (REQ-4.1, REQ-4.2, REQ-4.3, REQ-4.4) | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/design.md` | 마이크로서비스 분해 전략 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 5 |
| `/Users/axient/Documents/contextSet/layers/system/orchestration.md` | 지능적 라우팅 엔진 | 5 | 4 | 1 | 1 | 1 | 1 | 5 | 5 |
| `/Users/axient/Documents/contextSet/contexts/action/git-integration.md` | 복구 전략 | 5 | 5 | 0 | 1 | 0 | 1 | 4 | 4 |
| `/Users/axient/Documents/contextSet/tools/platforms/ios-platform.md` | RULE_2_1: 절대 리젝 방지 규칙 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/expertise/analyst/system-analysis.md` | 1. 발견 및 분류 단계 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |

## Verdict
- Status: PASS
- Reasons: none
