# Ownership Map

문제를 어디서 시작할지 빠르게 정하는 문서입니다.

## Top-Level Rule
- Core runtime 는 로컬 context runtime 을 직접 구성하는 경계만 소유한다.
- Ops/lab 는 benchmark, eval, trace, release, audit 같이 운영 증거와 실험 surface 를 소유한다.
- Vendored OM 은 `src/om/engine` 아래에 명시적으로 고립한다.
- Shared model/config 는 여러 경계가 함께 읽되, 도메인 정책은 각 소유 경계에서 결정한다.

## Core Runtime
- `src/client.rs`
- `src/fs.rs`
- `src/uri.rs`
- `src/ingest.rs`
- `src/index.rs`
- `src/index/*`
- `src/retrieval/*`
- `src/session/*`
- `src/state/*`
- `src/ontology/*`
- `src/om/mod.rs`
- `src/om/thread_identity.rs`
- `src/om/failure.rs`
- `src/om_bridge.rs`
- 책임: rooted filesystem, `axiom://`, `context.db`, retrieval, session lifecycle

## Ops / Lab
- `src/cli/*`
- `src/commands/*`
- `src/client/benchmark/*`
- `src/client/eval/*`
- `src/client/trace/*`
- `src/client/release/*`
- `src/client/request_log.rs`
- `src/client/queue_reconcile.rs`
- `src/client/mirror_outbox*`
- `src/release_gate.rs`
- `src/release_gate/*`
- `src/security_audit.rs`
- `scripts/quality_gates.sh`
- `scripts/release_pack_strict_gate.sh`
- `scripts/perf_regression_gate.sh`
- 책임: CLI, benchmark/eval, trace, audit, release gate

## Vendored OM
- `src/om/engine/*`
- 책임: pure contract, parser, prompt, transform, inference
- 규칙: runtime policy와 persistence policy는 두지 않는다.

## Shared Model / Config
- `src/models/*`
- `src/config/*`
- `src/error.rs`
- `src/embedding.rs`
- `src/parse.rs`
- `src/mime.rs`
- `src/text.rs`
- `src/jsonl.rs`
- `src/catalog.rs`
- `src/quality.rs`
- 책임: data contract, config, error, shared helper

## Change Routing

| Change type | Start here | Escalate to |
|---|---|---|
| URI / filesystem / state correctness | core runtime | shared model/config |
| search quality / restore / reindex / performance | core runtime | ops/lab for benchmark evidence |
| benchmark / eval / trace / release evidence | ops/lab | core runtime if hot path root cause exists |
| OM contract / transform semantics | vendored OM | core runtime only for integration seams |
| cross-cutting config or model shape | shared model/config | owning runtime or ops boundary |
