# Packages

이 저장소의 Rust package는 하나입니다. runtime library와 operator CLI binary를 같은 crate에 두고, web/mobile companion 프로젝트는 여기 넣지 않습니다.

## Package Map
- [`axiomsync`](./axiomsync/README.md): runtime library, operator CLI binary, persistence, retrieval, session, release evidence

## Out Of Repository
- web companion project
- mobile FFI companion project
- iOS and Android application shells

## Common Commands
```bash
cargo run -p axiomsync -- --help
process-compose --log-file logs/process-compose.log -f process-compose.yaml up
bash scripts/quality_gates.sh
```

## Reader Path
- Start with [../README.md](../README.md)
- Runtime boundary: [`axiomsync`](./axiomsync/README.md)
- Runtime and CLI boundary: [`axiomsync`](./axiomsync/README.md)
- Contracts and architecture: [../docs/README.md](../docs/README.md)
