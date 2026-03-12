# Docs

현재 유지하는 문서는 이 네 개면 충분합니다. 나머지는 crate README와 코드가 증거 원본입니다.

## Start Here
- [README.md](../README.md): 저장소가 무엇을 소유하는지, 어떻게 실행하는지, 어떤 gate로 릴리즈하는지
- [ARCHITECTURE.md](./ARCHITECTURE.md): 런타임 레이어, 데이터 흐름, 외부 프로젝트 경계
- [API_CONTRACT.md](./API_CONTRACT.md): URI, 저장소, 검색, 세션, 릴리즈 계약
- [../crates/README.md](../crates/README.md): runtime package map

## Read By Need
- Operator:
  Root README, `crates/axiomsync/README.md`
- Runtime developer:
  `ARCHITECTURE.md`, `API_CONTRACT.md`, `crates/axiomsync/README.md`
- Release owner:
  Root README, `API_CONTRACT.md`, `scripts/quality_gates.sh`, `scripts/release_pack_strict_gate.sh`

## Repository Boundary
- This repository owns the runtime library and CLI only.
- Web and mobile companion projects are external.
- Long-lived plan logs and historical rollout notes do not belong in `docs/`.

## Documentation Rules
- Stable runtime behavior is described in `API_CONTRACT.md`.
- Structural explanation belongs in `ARCHITECTURE.md`.
- Package-local detail belongs in each crate README.
- If a claim cannot be backed by code or scripts in this repo, it should not be promoted into a maintained doc.
