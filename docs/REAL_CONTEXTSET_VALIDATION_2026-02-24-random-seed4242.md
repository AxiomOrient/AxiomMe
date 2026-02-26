# Real Dataset Random Benchmark (contextSet)

Date: 2026-02-24
Seed: 4242
Root: `/tmp/axiomme-contextset-root-qB6Atn`
Dataset: `/Users/axient/Documents/contextSet`
Target URI: `axiom://resources/contextSet`
Raw data: `/Users/axient/repository/AxiomMe/docs/REAL_CONTEXTSET_VALIDATION_2026-02-24-random-seed4242.tsv`

## Ingest
- Status: ok
- Recursive entries listed: 107
- Tree root: axiom://resources/contextSet

## Random Retrieval Metrics
- Sampled heading scenarios: 24 (candidate headings: 1268)
- Unique headings in sample: 24 (ambiguous duplicates: 0)
- search min-match filter applied scenarios: 24/24 (min-match-tokens=2)
- find non-empty: 24/24 (100.00%)
- search non-empty: 24/24 (100.00%)
- find top1 expected-uri: 19/24 (79.17%)
- search top1 expected-uri: 19/24 (79.17%)
- find top5 expected-uri: 23/24 (95.83%)
- search top5 expected-uri: 23/24 (95.83%)

## Latency (ms)
- find mean/p50/p95: 6.54 / 6 / 7
- search mean/p50/p95: 6.08 / 6 / 8

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
| `/Users/axient/Documents/contextSet/design.md` | 4. 지식 관리 시스템 (REQ-4.1, REQ-4.2, REQ-4.3, REQ-4.4) | 5 | 5 | 1 | 1 | 1 | 1 | 19 | 8 |
| `/Users/axient/Documents/contextSet/design.md` | 마이크로서비스 분해 전략 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 5 |
| `/Users/axient/Documents/contextSet/layers/system/orchestration.md` | 지능적 라우팅 엔진 | 5 | 4 | 1 | 1 | 1 | 1 | 6 | 5 |
| `/Users/axient/Documents/contextSet/contexts/action/git-integration.md` | 복구 전략 | 5 | 5 | 0 | 1 | 0 | 1 | 4 | 4 |
| `/Users/axient/Documents/contextSet/tools/platforms/ios-platform.md` | RULE_2_1: 절대 리젝 방지 규칙 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/expertise/analyst/system-analysis.md` | 1. 발견 및 분류 단계 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/tools/languages/swift.md` | ANTIPATTERN_4: 메모리 누수 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/layers/task/task-management.md` | 도구 및 통합 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/tools/methodologies/tdd-methodology.md` | 레거시 코드에 TDD 도입 | 5 | 2 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/contexts/action/git-integration.md` | 4. 품질 검증 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/SYSTEM_OVERVIEW.md` | Phase 3: AI Integration & Performance (v2.0) | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 9 |
| `/Users/axient/Documents/contextSet/layers/task/testing.md` | 테스트 보고서 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 6 |
| `/Users/axient/Documents/contextSet/contexts/action/git-integration.md` | 2. 지능적 커밋 생성 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 6 |
| `/Users/axient/Documents/contextSet/tools/languages/python.md` | RULE_2_2: 개방-폐쇄 원칙 (OCP) | 5 | 5 | 0 | 1 | 0 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/INTEGRATION_GUIDE.md` | 3. LLM 에이전트 통합 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 7 |
| `/Users/axient/Documents/contextSet/layers/task/documentation.md` | 보안 고려사항 | 5 | 5 | 0 | 1 | 0 | 1 | 5 | 5 |
| `/Users/axient/Documents/contextSet/tools/languages/swift.md` | RULE_1_1: 자동화된 코드 품질 | 5 | 5 | 1 | 1 | 1 | 1 | 5 | 6 |
| `/Users/axient/Documents/contextSet/layers/task/task-management.md` | 사용 예시 | 5 | 5 | 0 | 0 | 0 | 0 | 4 | 4 |
| `/Users/axient/Documents/contextSet/tools/methodologies/ai-principles.md` | ARTICLE_10: 에러 처리 원칙 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/expertise/analyst/system-analysis.md` | 상세 분석 결과 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/tools/languages/python.md` | RULE_7_1: AAA 패턴과 단일 책임 | 5 | 5 | 0 | 1 | 0 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/layers/task/design-process.md` | GraphQL 설계 | 5 | 2 | 1 | 1 | 1 | 1 | 7 | 7 |

## Verdict
- Status: PASS
- Reasons: none
