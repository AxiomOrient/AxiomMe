# Plans

이 디렉터리는 stable runtime contract 가 아니라, 방향 결정과 실행 순서를 추적하는 planning 문서를 둡니다.

## Start Here
- [AxiomSync_01_Current_State_Review.md](./AxiomSync_01_Current_State_Review.md): 현재 구조와 병목을 어떤 증거로 판단했는지 정리
- [AxiomSync_02_Target_Blueprint.md](./AxiomSync_02_Target_Blueprint.md): SQLite 유지, core 단순화, FTS5/hybrid 우선이라는 목표 청사진
- [AxiomSync_03_Roadmap.md](./AxiomSync_03_Roadmap.md): phase, gate, 보류 기준을 포함한 중기 로드맵
- [AxiomSync_04_Implementation_Guide.md](./AxiomSync_04_Implementation_Guide.md): 바로 집행할 태스크와 성능/검색 구현 가이드
- [TASKS.md](./TASKS.md): 지금부터 실제로 어떤 순서로 진행할지 추적하는 execution tracker

## Current Decision
- SQLite 는 유지한다.
- external DB 교체는 보류한다.
- 검색 계층은 SQLite 내부의 FTS5/hybrid 강화가 먼저다.
- core runtime 과 ops/lab 경계를 더 선명하게 만든다.

## Current State
- working set `T01/T02/T03/T20` 은 완료됐다.
- current execution truth 는 [`TASKS.md`](./TASKS.md) 가 가진다.
- retrieval/runtime/compatibility boundary 의 stable 설명은 [`docs/RETRIEVAL_STACK.md`](../docs/RETRIEVAL_STACK.md) 에서 읽는다.

## Next Sequence
1. `T04` search restore / boot profile follow-up 선정
2. `T10` core vs ops 경계 추가 정리
3. `T11` facade surface slimming 후보 재검토
4. `T21/T22` hybrid retrieval 후속 wave 판단

## Directory Rules
- 안정 계약은 `docs/`에 두고, 실행 계획과 우선순위 판단은 `plans/`에 둔다.
- 완료 증거와 다음 액션은 `TASKS.md`에 갱신한다.
- 구현이 끝난 사실은 코드와 테스트가 증거이고, `plans/`는 그 증거를 가리키는 추적 문서만 유지한다.
