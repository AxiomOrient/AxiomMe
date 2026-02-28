# axiomme-mobile-ffi

Minimal mobile FFI boundary for `axiomme-core`.

## Scope

- Export a stable C ABI for iOS/Swift integration.
- Keep data contracts explicit (`code + owned bytes payload`).
- Keep side effects isolated behind runtime handle methods.
- Be consumed by external mobile app projects (for example, a companion mobile app repository).

## Exported Surface

- `axiomme_runtime_new(root_dir, out_runtime)`
- `axiomme_runtime_initialize(runtime)`
- `axiomme_runtime_backend_status_json(runtime)`
- `axiomme_runtime_free(runtime)`
- `axiomme_owned_bytes_free(bytes)`

## Data Contract

- Every call returns `AxiommeFfiResult`:
  - `code`: `ok | invalid_argument | runtime_error`
  - `payload`: UTF-8 JSON bytes (success payload or structured error payload)
- Returned byte buffers are Rust-owned and must be released with
  `axiomme_owned_bytes_free`.

Header and Swift sample:
- C header: `include/axiomme_mobile_ffi.h`
- Swift wrapper sample: `examples/swift/AxiommeMobileFFI.swift`
  - Note: Swift imports runtime handle type as `OpaquePointer`.

## Build

```bash
cargo build -p axiomme-mobile-ffi
```

This crate links `axiomme-core` with `default-features = false`, so host-tool execution
(`host-tools` feature) is excluded from mobile builds.

`crate-type = ["staticlib", "cdylib"]` is enabled for native mobile linking.
