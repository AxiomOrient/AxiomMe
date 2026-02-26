# Real Dataset Random Benchmark (contextSet)

Date: 2026-02-24
Seed: 777
Root: `/tmp/axiomme-contextset-root-nBS8CE`
Dataset: `/Users/axient/Documents/contextSet`
Target URI: `axiom://resources/contextSet`
Raw data: `/Users/axient/repository/AxiomMe/docs/REAL_CONTEXTSET_VALIDATION_2026-02-24-random-seed777.tsv`

## Ingest
- Status: ok
- Recursive entries listed: 107
- Tree root: axiom://resources/contextSet

## Random Retrieval Metrics
- Sampled heading scenarios: 24 (candidate headings: 1268)
- Unique headings in sample: 24 (ambiguous duplicates: 0)
- search min-match filter applied scenarios: 23/24 (min-match-tokens=2)
- find non-empty: 24/24 (100.00%)
- search non-empty: 24/24 (100.00%)
- find top1 expected-uri: 19/24 (79.17%)
- search top1 expected-uri: 19/24 (79.17%)
- find top5 expected-uri: 22/24 (91.67%)
- search top5 expected-uri: 22/24 (91.67%)

## Latency (ms)
- find mean/p50/p95: 6.75 / 7 / 7
- search mean/p50/p95: 6.21 / 6 / 8

## CRUD Validation
- Create uri: `axiom://resources/contextSet/manual-crud/auto-crud-777.md`
- Create status: ok
- Update status: ok
- Read-back contains update token: pass (`crud-update-777`)
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
| `/Users/axient/Documents/contextSet/layers/domain/software-engineering-fundamentals.md` | 테스트 가능성 (Testability) | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 6 |
| `/Users/axient/Documents/contextSet/combinations/development/web-svelte-typescript.md` | 3. 타입 안전한 API 클라이언트 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/tools/platforms/backend-platform.md` | RULE_1_1: RESTful API 설계 원칙 (업계 표준) | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 8 |
| `/Users/axient/Documents/contextSet/requirements.md` | 승인 기준 | 5 | 5 | 1 | 1 | 1 | 1 | 5 | 4 |
| `/Users/axient/Documents/contextSet/layers/task/documentation.md` | 다국어 지원 | 5 | 3 | 0 | 1 | 0 | 1 | 7 | 6 |
| `/Users/axient/Documents/contextSet/tools/platforms/web-platform.md` | RULE_3_2: 성능 제약사항 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/tools/languages/python.md` | RULE_8_3: 비동기 성능 최적화 | 5 | 5 | 1 | 1 | 1 | 1 | 19 | 6 |
| `/Users/axient/Documents/contextSet/tools/methodologies/tdd-methodology.md` | 테스트 격리 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 6 |
| `/Users/axient/Documents/contextSet/layers/system/agent-communication-style.md` | 기술적 커뮤니케이션 | 5 | 5 | 1 | 1 | 1 | 1 | 5 | 5 |
| `/Users/axient/Documents/contextSet/combinations/development/ai-python-pytorch.md` | 고급 훈련 시스템 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 7 |
| `/Users/axient/Documents/contextSet/layers/system/workflow-management.md` | 2. 성능 모니터링 및 분석 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 7 |
| `/Users/axient/Documents/contextSet/layers/domain/software-engineering-fundamentals.md` | 핵심 개발 원칙 (Core Development Principles) | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 8 |
| `/Users/axient/Documents/contextSet/combinations/development/mobile-android-compose.md` | Jetpack Compose UI 구현 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/README.md` | 원칙 | 5 | 5 | 1 | 1 | 1 | 1 | 2 | 2 |
| `/Users/axient/Documents/contextSet/layers/system/workflow-management.md` | 고급 사용법 | 5 | 5 | 0 | 0 | 0 | 0 | 6 | 6 |
| `/Users/axient/Documents/contextSet/layers/task/testing.md` | 3. 테스트 실행 및 모니터링 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/tools/languages/rust-cargo-workspace.md` | 품질 검사 | 5 | 5 | 0 | 0 | 0 | 0 | 5 | 4 |
| `/Users/axient/Documents/contextSet/tools/languages/typescript.md` | RULE_3_3: 제네릭 활용 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/combinations/development/mobile-android-compose.md` | Data Layer | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 8 |
| `/Users/axient/Documents/contextSet/contexts/action/refactoring.md` | Refactoring Workflow | 5 | 3 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/expertise/analyst/system-analysis.md` | 성능 메트릭 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/contexts/action/refactoring.md` | 안전성 보장 | 5 | 4 | 0 | 1 | 0 | 1 | 7 | 6 |
| `/Users/axient/Documents/contextSet/tools/platforms/macos-platform.md` | SECTION 5: 시스템 통합 및 생태계 요구사항 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/contexts/action/building.md` | Web (TypeScript/React) | 5 | 5 | 0 | 1 | 0 | 1 | 6 | 7 |

## Verdict
- Status: PASS
- Reasons: none
