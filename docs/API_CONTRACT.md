# API Contract

## 0. Scope
이 문서는 현재 저장소가 보장하는 안정 계약만 정의합니다. 구현 세부, 실험 옵션, 외부 companion 프로젝트 설계는 제외합니다.

## 1. Repository Boundary
- This repository owns the runtime library and operator CLI only.
- Web viewer/server and mobile FFI are companion projects outside this repository.

## 2. URI Contract
- Canonical URI: `axiom://{scope}/{path}`
- Core scopes: `resources`, `user`, `agent`, `session`
- Internal scopes: `temp`, `queue`
- `queue` scope는 시스템 작업 외 쓰기 금지

## 3. Persistence Contract
- Canonical local store: `<root>/context.db`
- `context.db`는 큐, 체크포인트, OM 상태, 검색 영속 상태를 함께 저장한다.
- 런타임 검색은 메모리 인덱스로 수행하되, 부팅 시 persisted search state에서 복원한다.
- 런타임은 legacy DB 파일명을 탐색하거나 자동 마이그레이션하지 않는다.
- Persistence backend는 SQLite로 고정한다.

## 4. Retrieval Contract
- Public query surface:
  - `find(query, target_uri?, limit?, score_threshold?, filter?)`
  - `search(query, target_uri?, session?, limit?, score_threshold?, filter?)`
  - `search_with_request(SearchRequest { ..., runtime_hints })`
- Runtime retrieval backend policy는 `memory_only`다.
- `AXIOMSYNC_RETRIEVAL_BACKEND=memory`만 허용된다.
- `sqlite`, `bm25`, unknown retrieval backend values는 configuration error로 거부된다.

## 5. Filesystem And Resource Contract
- `initialize()`
- `add_resource(path_or_url, target?, reason?, instruction?, wait, wait_mode?, timeout?)`
- `wait_processed(timeout?)`
- `ls(uri, recursive, simple)`
- `read(uri)`
- `mkdir(uri)`
- `rm(uri, recursive)`
- `mv(from_uri, to_uri)`

## 6. Session And Memory Contract
- `session(session_id?)`
- `sessions()`
- `delete(session_id)`
- `promote_session_memories(request)`
- `checkpoint_session_archive_only(session_id)`

## 7. OM v2 Boundary Contract
- Pure OM contract and transform 계층은 vendored engine 아래에 유지한다.
- Runtime and persistence policy 계층은 `axiomsync`가 담당한다.
- Prompt and response header strict fields:
  - `contract_name`
  - `contract_version`
  - `protocol_version`
- XML/JSON fallback content도 contract marker 검증을 통과해야 수용된다.
- Search hint는 OM snapshot read-model 기준으로 구성한다.

## 8. Release Gate Contract
- Repository-grade checks:
  - `bash scripts/quality_gates.sh`
  - `bash scripts/release_pack_strict_gate.sh --workspace-dir <repo>`
- Contract integrity gate는 다음을 검증한다:
  - contract execution probe
  - episodic API probe
  - prompt signature version-bump policy
  - ontology contract probe
- `HEAD~1` 미존재, shallow history, path rename/cutover 등으로 이전 정책 소스를 읽을 수 없을 때는 current workspace policy shape 검증으로 fallback 한다.

## 9. Dependency Contract
- `axiomsync` must not declare an `episodic` crate dependency.
- Required vendored contract file: `crates/axiomsync/src/om/engine/prompt/contract.rs`
- Required vendored engine entry: `crates/axiomsync/src/om/engine/mod.rs`
- `Cargo.lock` must not resolve an `episodic` package for `axiomsync`.

## 10. Non-goals
- Web viewer implementation detail
- Mobile FFI surface design
- Experimental benchmark internals
- Historical rollout logs

## 11. Canonical Reference
- [Architecture](./ARCHITECTURE.md)
