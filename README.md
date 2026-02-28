# AxiomMe

**Production-grade Context Management System for Agentic Runtimes**

AxiomMe는 에이전트 환경을 위한 Rust 기반의 로컬 컨텍스트 관리 시스템입니다. `axiom://` 가상 파일 시스템을 통해 데이터의 일관성을 보장하며, 고성능 검색 엔진과 정밀한 세션 메모리 관리 기능을 제공합니다.

## Key Features
- **Local-first Architecture**: 모든 데이터는 로컬 디스크(`fs`), SQLite(`state`), 그리고 인메모리 인덱스(`index`)에서 관리됩니다.
- **High Performance Retrieval**: John Carmack의 성능 철학을 반영한 최적화된 DRR(Document Retrieval and Ranking) 엔진을 탑재하여 수만 개의 문서에서도 밀리초 단위의 검색을 보장합니다.
- **Atomic Memory Promotion**: 세션의 대화 내용을 분석하여 유의미한 기억으로 승격시키는 과정을 체크포인트 기반의 원자적 작업으로 수행합니다.
- **Rigorous Quality Gates**: 600개 이상의 테스트와 다층 품질 게이트를 통해 상용 수준의 안정성을 유지합니다.

## Quick Start
```bash
# 초기화
axiomme init

# 리소스 추가
axiomme add ./docs --target axiom://resources/docs

# 검색
axiomme search "oauth flow"

# 세션 커밋 및 기억 승격
axiomme session commit
```

## Documentation
- [Architecture](./docs/ARCHITECTURE.md): 시스템 설계 및 데이터 흐름 명세
- [API Contract](./docs/API_CONTRACT.md): 인터페이스 규약 및 데이터 모델
- [Feature Spec](./docs/FEATURE_SPEC.md): 기능적 인바리언트 보장 항목
- [Ontology Policy](./docs/ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md): 데이터 스키마 진화 정책
