# Real Dataset Random Benchmark (contextSet)

Date: 2026-02-24
Seed: 9001
Root: `/tmp/axiomme-contextset-root-uwm4je`
Dataset: `/Users/axient/Documents/contextSet`
Target URI: `axiom://resources/contextSet`
Raw data: `/Users/axient/repository/AxiomMe/docs/REAL_CONTEXTSET_VALIDATION_2026-02-24-random-seed9001.tsv`

## Ingest
- Status: ok
- Recursive entries listed: 107
- Tree root: axiom://resources/contextSet

## Random Retrieval Metrics
- Sampled heading scenarios: 24 (candidate headings: 1268)
- Unique headings in sample: 24 (ambiguous duplicates: 0)
- search min-match filter applied scenarios: 21/24 (min-match-tokens=2)
- find non-empty: 24/24 (100.00%)
- search non-empty: 24/24 (100.00%)
- find top1 expected-uri: 17/24 (70.83%)
- search top1 expected-uri: 17/24 (70.83%)
- find top5 expected-uri: 22/24 (91.67%)
- search top5 expected-uri: 22/24 (91.67%)

## Latency (ms)
- find mean/p50/p95: 6.08 / 6 / 8
- search mean/p50/p95: 6.21 / 6 / 8

## CRUD Validation
- Create uri: `axiom://resources/contextSet/manual-crud/auto-crud-9001.md`
- Create status: ok
- Update status: ok
- Read-back contains update token: pass (`crud-update-9001`)
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
| `/Users/axient/Documents/contextSet/tools/methodologies/ai-pair-programming.md` | Phase 3: AI 구현 요청 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/layers/task/documentation.md` | 성능 최적화 | 5 | 5 | 0 | 0 | 0 | 0 | 6 | 6 |
| `/Users/axient/Documents/contextSet/layers/system/agent-communication-style.md` | 문제 해결 | 5 | 5 | 1 | 1 | 1 | 1 | 5 | 5 |
| `/Users/axient/Documents/contextSet/tools/methodologies/tdd-methodology.md` | SECTION 4: 필수 작업 절차 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 7 |
| `/Users/axient/Documents/contextSet/INTEGRATION_GUIDE.md` | 트러블슈팅 | 5 | 5 | 1 | 1 | 1 | 1 | 4 | 4 |
| `/Users/axient/Documents/contextSet/tools/methodologies/ai-pair-programming.md` | Phase 1: 타입 정의 (Type-First Design) | 5 | 5 | 1 | 1 | 1 | 1 | 8 | 8 |
| `/Users/axient/Documents/contextSet/tools/frameworks/actix-web.md` | SECTION 6: HTTP 핸들러 | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/tools/languages/kotlin.md` | SECTION 3: 타입 시스템 및 네이밍 | 5 | 5 | 0 | 1 | 0 | 1 | 6 | 8 |
| `/Users/axient/Documents/contextSet/layers/domain/software-engineering-fundamentals.md` | DRY (Don't Repeat Yourself) | 5 | 2 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/layers/system/orchestration.md` | Orchestration Workflow | 5 | 2 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/layers/task/task-management.md` | 목적 | 5 | 5 | 0 | 0 | 0 | 0 | 4 | 3 |
| `/Users/axient/Documents/contextSet/tools/languages/typescript.md` | TypeScript 개발 가이드라인 (Production-Grade) | 5 | 5 | 1 | 1 | 1 | 1 | 8 | 8 |
| `/Users/axient/Documents/contextSet/layers/task/debugging.md` | AI 기반 오류 분석 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/tools/platforms/web-platform.md` | SECTION 0: 문서 범위 및 지침 | 5 | 5 | 0 | 1 | 0 | 1 | 5 | 5 |
| `/Users/axient/Documents/contextSet/contexts/action/building.md` | Building Workflow | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 7 |
| `/Users/axient/Documents/contextSet/combinations/development/backend-rust-axum.md` | 🔐 인증 미들웨어 | 5 | 3 | 1 | 1 | 1 | 1 | 6 | 5 |
| `/Users/axient/Documents/contextSet/layers/task/testing.md` | 지능형 테스트 최적화 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/layers/system/workflow-management.md` | 승인 프로세스 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/combinations/development/backend-rust-axum.md` | 🧪 테스트 전략 | 5 | 5 | 1 | 1 | 1 | 1 | 6 | 6 |
| `/Users/axient/Documents/contextSet/tools/languages/rust-cargo-workspace.md` | 전체 워크스페이스 | 5 | 1 | 1 | 1 | 1 | 1 | 5 | 6 |
| `/Users/axient/Documents/contextSet/SYSTEM_OVERVIEW.md` | 🚀 User Workspace Structure | 5 | 5 | 1 | 1 | 1 | 1 | 7 | 6 |
| `/Users/axient/Documents/contextSet/layers/task/documentation.md` | 오류 처리 및 복구 | 5 | 5 | 0 | 1 | 0 | 1 | 6 | 7 |
| `/Users/axient/Documents/contextSet/expertise/architect/security-architect.md` | 정체성 | 5 | 5 | 0 | 1 | 0 | 1 | 5 | 5 |
| `/Users/axient/Documents/contextSet/layers/task/testing.md` | CI/CD 통합 | 5 | 5 | 0 | 1 | 0 | 1 | 6 | 7 |

## Verdict
- Status: PASS
- Reasons: none
