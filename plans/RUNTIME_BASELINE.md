# Runtime Baseline

`T02`의 목적은 boot / reindex / search / queue 지표를 추측이 아니라 재현 가능한 숫자로 고정하는 것입니다.

## Entrypoint

```bash
cargo run -p axiomsync --bin runtime_baseline -- \
  --json-out plans/runtime-baseline.json \
  --markdown-out plans/runtime-baseline.md
```

## Scenarios
- `small`
- `medium`
- `stress`

## Metrics
- cold boot
- warm boot
- full reindex
- first search latency
- steady-state p50/p95 search
- queue replay throughput

## Evidence Rule
- JSON 은 machine-readable baseline 이다.
- markdown 은 사람 검토용 요약이다.
- 성능 작업은 이 출력과 전/후를 비교해서만 주장한다.
