# Docs

읽을 문서는 아래면 충분합니다.

## Start Here
- [README.md](../README.md): 저장소 요약
- [API_CONTRACT.md](./API_CONTRACT.md): 안정 계약
- [ARCHITECTURE.md](./ARCHITECTURE.md): 구조와 경계
- [RETRIEVAL_STACK.md](./RETRIEVAL_STACK.md): 검색 경로
- [OWNERSHIP_MAP.md](./OWNERSHIP_MAP.md): 변경 시작점
- [BUILD_ARTIFACT_CONTROL.md](./BUILD_ARTIFACT_CONTROL.md): 빌드 산출물 운영
- [../plans/README.md](../plans/README.md): 계획과 실행 순서

## Read By Need
- Operator:
  `README.md`
- Runtime developer:
  `API_CONTRACT.md`, `ARCHITECTURE.md`, `RETRIEVAL_STACK.md`, `OWNERSHIP_MAP.md`
- Release owner:
  `README.md`, `API_CONTRACT.md`, `scripts/quality_gates.sh`, `scripts/release_pack_strict_gate.sh`
- Rust workspace operator:
  `BUILD_ARTIFACT_CONTROL.md`, `scripts/quality_gates.sh`, `Cargo.toml`

## Repository Boundary
- This repository owns the runtime library and CLI only.
- Web and mobile companion projects are external.
- 계획과 실행 기록은 [`plans/`](../plans/README.md)에 둔다.

## Documentation Rules
- 계약은 `API_CONTRACT.md`
- 구조는 `ARCHITECTURE.md`
- 검색은 `RETRIEVAL_STACK.md`
- 소유 경계는 `OWNERSHIP_MAP.md`
- 계획과 실험 rationale 은 `plans/`
