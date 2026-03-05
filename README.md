# AxiomMe

**Production-grade Context Management System for Agentic Runtimes**

AxiomMe는 에이전트 환경을 위한 Rust 기반 로컬 컨텍스트 런타임입니다.
`axiom://` URI를 기준으로 파일, 상태, 검색, 세션 메모리를 일관된 계약으로 다룹니다.

## Overview
- Local-first: `fs + sqlite + in-memory index`
- Deterministic retrieval: `find/search` + trace metadata
- Session/OM memory flow: checkpointed promotion and replay-safe updates
- Release safety: contract/reliability/security gates

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

## Repository Structure
- [crates/README.md](./crates/README.md): crate index
- [docs/README.md](./docs/README.md): canonical docs index

## Core Modules
- [crates/axiomme-core](./crates/axiomme-core/README.md): runtime/data engine
- [crates/axiomme-cli](./crates/axiomme-cli/README.md): CLI surface
- [crates/axiomme-mobile-ffi](./crates/axiomme-mobile-ffi/README.md): mobile FFI boundary

## Guides
- [Architecture](./docs/ARCHITECTURE.md)
- [API Contract](./docs/API_CONTRACT.md)
- [Ontology Evolution Policy](./docs/ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md)

## Operations/Quality
```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit -q
```

## Constraints
- canonical URI protocol: `axiom://`
- runtime code and pure OM contract boundary must remain explicit (`axiomme-core` vs `episodic`)
