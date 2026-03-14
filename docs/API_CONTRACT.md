# API Contract

이 문서는 저장소가 보장하는 안정 계약만 적습니다.

## Repository Boundary
- This repository owns the runtime library and operator CLI only.
- Web viewer/server and mobile FFI are companion projects outside this repository.

## URI Contract
- Canonical URI: `axiom://{scope}/{path}`
- Core scopes: `resources`, `user`, `agent`, `session`
- Internal scopes: `temp`, `queue`
- `queue` scope는 시스템 작업 외 쓰기 금지

## Persistence Contract
- Canonical local store: `<root>/context.db`
- `context.db`는 큐, 체크포인트, OM 상태, 검색 영속 상태를 함께 저장한다.
- 런타임 검색은 메모리 인덱스로 수행하되, 부팅 시 persisted search state에서 복원한다.
- 런타임은 legacy DB 파일명을 탐색하거나 자동 마이그레이션하지 않는다.
- known in-place compatibility repair 는 `context.db` 내부 schema/bootstrap marker 기준으로만 수행한다.
- Persistence backend는 SQLite로 고정한다.

## Retrieval Contract
- Public query surface:
  - `find(query, target_uri?, limit?, score_threshold?, filter?)`
  - `search(query, target_uri?, session?, limit?, score_threshold?, filter?)`
  - `search_with_request(SearchRequest { ..., runtime_hints })`
- Runtime retrieval backend policy는 `memory_only`다.
- persisted lexical projection 으로 SQLite `search_docs` / `search_docs_fts` 를 유지할 수 있지만, runtime ranking contract 는 메모리 인덱스가 담당한다.
- `search_docs_fts` bootstrap completeness 는 `system_kv` marker/version 으로 추적할 수 있고, marker 가 없으면 rebuild 가 재시도된다.
- `FindResult.query_results` 와 `hit_buckets` 가 canonical retrieval result shape 다.
- `FindResult.memories`, `resources`, `skills` 는 canonical source 가 아니라 backward-compat derived view 다.
- `AXIOMSYNC_RETRIEVAL_BACKEND=memory`만 허용된다.
- `sqlite`, `bm25`, unknown retrieval backend values는 configuration error로 거부된다.

## Filesystem And Resource Contract
- `initialize()`
- `add_resource(path_or_url, target?, reason?, instruction?, wait, wait_mode?, timeout?)`
- `wait_processed(timeout?)`
- `ls(uri, recursive, simple)`
- `read(uri)`
- `mkdir(uri)`
- `rm(uri, recursive)`
- `mv(from_uri, to_uri)`

## Session And Memory Contract
- `session(session_id?)`
- `sessions()`
- `delete(session_id)`
- `promote_session_memories(request)`
- `checkpoint_session_archive_only(session_id)`

## OM Boundary Contract
- Pure OM contract and transform 계층은 vendored engine 아래에 유지한다.
- Runtime and persistence policy 계층은 `axiomsync`가 담당한다.
- Prompt and response header strict fields:
  - `contract_name`
  - `contract_version`
  - `protocol_version`
- XML/JSON fallback content도 contract marker 검증을 통과해야 수용된다.
- Search hint는 OM snapshot read-model 기준으로 구성한다.

## Release Gate Contract
- Repository-grade checks:
  - `bash scripts/quality_gates.sh`
  - `bash scripts/release_pack_strict_gate.sh --workspace-dir <repo>`
- Contract integrity gate는 다음을 검증한다:
  - contract execution probe
  - episodic API probe
  - prompt signature version-bump policy
  - ontology contract probe
- `HEAD~1` 미존재, shallow history, path rename/cutover 등으로 이전 정책 소스를 읽을 수 없을 때는 current workspace policy shape 검증으로 fallback 한다.

## Dependency Contract
- `axiomsync` must not declare an `episodic` crate dependency.
- Required vendored contract file: `crates/axiomsync/src/om/engine/prompt/contract.rs`
- Required vendored engine entry: `crates/axiomsync/src/om/engine/mod.rs`
- `Cargo.lock` must not resolve an `episodic` package for `axiomsync`.

## Non-goals
- Web viewer implementation detail
- Mobile FFI surface design
- Experimental benchmark internals
- Historical rollout logs

## References
- [Architecture](./ARCHITECTURE.md)
- [Retrieval Stack](./RETRIEVAL_STACK.md)
