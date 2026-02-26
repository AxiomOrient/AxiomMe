# Real Dataset Random Benchmark (contextSet)

Date: 2026-02-24
Seed: 20260224
Root: `/tmp/axiomme-contextset-root-jjLNui`
Dataset: `/Users/axient/Documents/contextSet`
Target URI: `axiom://resources/contextSet`
Raw data: `docs/REAL_CONTEXTSET_VALIDATION_2026-02-24-random-recheck.tsv`

## Ingest
- Status: ok
- Recursive entries listed: 107
- Tree root: axiom://resources/contextSet

## Random Retrieval Metrics
- Sampled heading scenarios: 16 (candidate headings: 1622)
- Unique headings in sample: 16 (ambiguous duplicates: 0)
- search min-match filter applied scenarios: 16/16 (min-match-tokens=2)
- find non-empty: 16/16 (100.00%)
- search non-empty: 16/16 (100.00%)
- find top1 expected-uri: 14/16 (87.50%)
- search top1 expected-uri: 14/16 (87.50%)
- find top5 expected-uri: 16/16 (100.00%)
- search top5 expected-uri: 16/16 (100.00%)

## Latency (ms)
- find mean/p50/p95: 6.25 / 6 / 7
- search mean/p50/p95: 6.44 / 6 / 8

## CRUD Validation
- Create uri: `axiom://resources/contextSet/manual-crud/auto-crud-20260224.md`
- Create status: ok
- Update status: ok
- Read-back contains update token: pass (`crud-update-20260224`)
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
| `/Users/axient/Documents/contextSet/tools/languages/python.md` | RULE_3_2: Pydantic 모델 활용 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 8 |
| `/Users/axient/Documents/contextSet/combinations/development/backend-rust-axum.md` | 🎯 핸들러 구현 | 5 | 2 | 1 | 1 | 1 | 1 | 5 | 6 |
| `/Users/axient/Documents/contextSet/layers/task/code-implementation.md` | 점진적 기능 추가 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/design.md` | 데이터 모델 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/contexts/action/building.md` | 6. 패키징 및 배포 준비 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 7 |
| `/Users/axient/Documents/contextSet/tools/languages/swift-testing.md` | 테스트 스위트 구조 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/layers/task/design-process.md` | 컴포넌트 설계 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/combinations/development/mobile-ios-swiftui.md` | File > New > Project > iOS > App | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/layers/system/orchestration.md` | 관련 워크플로우 | 5 | 5 | 0 | 1 | 0 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/combinations/development/mobile-ios-swiftui.md` | 의존성 관리 (Swift Package Manager) | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/layers/domain/software-engineering-fundamentals.md` | 유지보수성 (Maintainability) | 5 | 1 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/expertise/architect/security-architect.md` | Rust 보안 모범 사례 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 7 |
| `/Users/axient/Documents/contextSet/layers/task/debugging.md` | 디버깅 전략 | 5 | 4 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/combinations/development/web-svelte-typescript.md` | TypeScript 체크 | 5 | 3 | 0 | 1 | 0 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/layers/system/workflow-management.md` | 보안 및 규정 준수 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 6 |
| `/Users/axient/Documents/contextSet/layers/system/orchestration.md` | 리소스 최적화 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |

## Verdict
- Status: PASS
- Reasons: none
