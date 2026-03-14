# Build Artifact Control

`target/`이 비정상적으로 커질 때 보는 운영 문서입니다.

## When To Use This
- `target/debug/deps`에 큰 해시 파일이 수백 개 생김
- `target/debug/incremental`이 수백 MB 이상 커짐
- `cargo test --workspace` 한 번으로 `target/debug`가 1GB 이상 증가
- `*.rcgu.o`, `*.rlib`, 해시가 붙은 test/bin 실행 파일이 반복 생성됨

## Measure First
```bash
cargo clean
cargo test --workspace --quiet >/dev/null

du -sh target/debug target/debug/deps target/debug/incremental target/debug/build
find target/debug/deps -maxdepth 1 -type f -size +20M -print0 | xargs -0 ls -lh | sort -k5 -h | tail -40
find target/debug/deps -maxdepth 1 -type f -name '*.rcgu.o' | wc -l
```

## Read The Artifacts
- `*.rcgu.o`: Rust CodeGen Unit object file
- `lib*.rlib`: crate archive
- `*.rmeta`: metadata artifact
- 해시가 붙은 실행 파일: bin/test target 링크 결과
- `target/debug/incremental`: incremental compilation cache

## What Worked Here
- 중복 cargo 실행 제거
- dev/test profile 축소

```toml
[profile.dev]
debug = 0
incremental = false
codegen-units = 64

[profile.test]
debug = 0
incremental = false
codegen-units = 64
```

- `debug = 0`
  debug 심볼 제거
- `incremental = false`
  incremental cache 제거
- `codegen-units = 64`
  giant crate의 shard churn 완화

## Before / After From This Repository
```bash
cargo clean
cargo test --workspace --quiet >/dev/null
```

결과:

- `target/debug`: `1.5G` -> `606M`
- `target/debug/deps`: `991M` -> `510M`
- `target/debug/incremental`: `397M` -> `0B`
- `target/debug/build`: `66M` -> `56M`
- largest `libaxiomsync-*.rlib`: `70M` -> `29M`
- largest `axiomsync-*` binaries: `44M` -> `36~37M`
- visible `axiomsync-*.rcgu.o`: `923` -> `0`

## Reusable Rollout Order
1. `cargo clean` 후 대표 명령 한 개로 baseline을 잡는다.
2. `deps`와 `incremental` 중 어디가 큰지 먼저 본다.
3. 스크립트가 같은 `cargo build/test/clippy/run`을 중복 호출하는지 제거한다.
4. 그래도 크면 `profile.dev`, `profile.test`에서 `debug = 0`, `incremental = false`를 검토한다.
5. 그래도 크면 crate 분해로 넘어간다.

## Tradeoffs
- `incremental = false`: 디스크 사용량 감소, 반복 로컬 재빌드 느려질 수 있음
- `debug = 0`: artifact 크기 감소, 로컬 디버깅 정보 감소

## Verification Contract
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/quality_gates.sh
du -sh target/debug target/debug/deps target/debug/incremental target/debug/build
```
