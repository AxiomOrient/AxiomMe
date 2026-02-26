# Engineering Playbook: Mobile-Native + FFI + Runtime Performance

Date: 2026-02-23
Scope: AxiomMe core/CLI/FFI and iOS native integration workflow

## Purpose

This document captures failures and corrections from recent work and turns them into reusable engineering rules.
The goal is simple: keep data contracts explicit, isolate side effects, and avoid hidden performance costs.

## Self Feedback and Corrections

1. I over-focused on core-side lock contention before proving the UI path.  
   Correction: iOS stutter was primarily caused by main-thread FFI work and recursive list behavior after seed.
2. I treated iOS warning remediation as complete too early.  
   Correction: deployment-target mismatch warnings still appeared after the first script pass and must be treated as unresolved until fully clean.
3. I mixed "tests pass" with "release-ready" language.  
   Correction: passing tests are necessary but not sufficient; release readiness also requires clean packaging constraints (SDK target alignment, boundary clarity, host/mobile separation).

## Incident Log (Symptom -> Cause -> Fix -> Rule)

### 1) iOS folder navigation stutter after sample seed

- Symptom: after `init` + sample file seed, moving to another folder caused visible UI pauses.
- Cause:
  - synchronous runtime/FFI calls in UI execution path,
  - recursive listing enabled by default right after seed.
- Fix:
  - split UI state and runtime effects (`@MainActor` state + background runtime worker),
  - default to non-recursive listing for normal navigation.
- Rule: UI thread owns rendering only. I/O, indexing, and FFI stay off-main by default.

### 2) Observer batch execution risked unbounded thread growth

- Symptom: batch count growth could spawn many blocking workers and increase tail latency.
- Cause: per-batch thread creation without an explicit upper bound.
- Fix: apply explicit parallel cap (`MAX_OBSERVER_BATCH_PARALLELISM`) and bounded chunk execution.
- Rule: concurrency must be budgeted as data, not implied by input size.

### 3) Mobile integration was blocked by missing explicit ABI boundary

- Symptom: core runtime existed but iOS consumption boundary was unclear.
- Cause: no dedicated, stable C ABI contract was the primary interface.
- Fix: dedicated `axiomme-mobile-ffi` crate with explicit result envelope (`code + owned bytes payload`), explicit alloc/free ownership, and `staticlib/cdylib`.
- Rule: for cross-language runtime use, define data and ownership contracts first, then expose behavior.

### 4) Host-only release/security gates leaked into runtime concerns

- Symptom: mobile contexts cannot run host subprocess-based gates (`cargo check/fmt/clippy/test`).
- Cause: host operational checks and runtime capabilities were not clearly separated.
- Fix: isolate host-only operations behind host tooling paths/features; mobile FFI depends on core with host features disabled.
- Rule: build-time/CI controls and runtime controls must be split at compile-time boundaries.

### 5) Web runtime coupling blurred delivery boundaries

- Symptom: packaging and lifecycle assumptions became mixed across CLI/mobile/web.
- Cause: runtime and viewer concerns were not strictly separated at the artifact level.
- Fix: keep web viewer as external delivery unit and keep this repository focused on core/CLI/FFI contracts.
- Rule: separate deployables by lifecycle model (CLI process, mobile app lifecycle, web server lifecycle).

### 6) iOS linker warnings from deployment-target mismatch

- Symptom: linker warnings showing objects built for newer iOS than app link target.
- Cause: Rust/C/C++ and Xcode deployment targets were not perfectly aligned.
- Fix: propagate minimum target flags consistently in XCFramework build scripts and Xcode target settings.
- Rule: deployment target is a contract; set it once and enforce consistently across all toolchains.

## Reusable Technical Rules

1. Model first:
   - keep stable structs/envelopes for boundary data (`request`, `response`, `error`, `ownership`),
   - avoid implicit map-shaped payloads at ABI boundaries.
2. Pure transforms first:
   - parsing, scoring, ranking, normalization should stay pure,
   - side effects (disk/network/process spawn) must be isolated and named by operation.
3. Explicit side-effect lanes:
   - UI/main lane: state mutation for rendering only,
   - worker lane: FFI, file I/O, indexing, network, queue replay.
4. Bounded concurrency:
   - every fan-out has a hard cap,
   - thread/task count scales by configured budget, not by raw input length.
5. Default cheap operations:
   - non-recursive listing as default navigation path,
   - recursive and full reindex only as explicit actions.
6. Ownership clarity:
   - every allocated buffer crossing FFI has one matching free function,
   - document lifetime and thread-safety assumptions in exported APIs.

## Verification Checklist (Generic but Concrete)

1. Correctness:
   - CRUD path test: create/read/update/delete across multiple directories,
   - index consistency test: write -> reindex -> search/read deterministic results.
2. Performance:
   - measure p50/p95 for list/read/save with sample and real datasets,
   - verify no main-thread blocking during navigation and editing.
3. Boundary integrity:
   - ABI schema compatibility test for success/error payloads,
   - ownership test for allocate/free cycles (no leaks, no double free).
4. Packaging:
   - simulator + device build/test both green,
   - no deployment-target mismatch warnings at release gate.
5. Operational split:
   - host-only release/security tasks executed in CI/desktop path only,
   - mobile runtime path excludes subprocess-dependent gates.

6. Real-use retrieval benchmark:
   - run random heading scenarios against real corpus (`contextSet`) with reproducible seed,
   - require explicit retrieval quality + latency report (`top1/top5`, `non-empty`, `p50/p95`) plus CRUD proof.

## Practical Command Baseline

```bash
# Core/CLI quality baseline
bash scripts/quality_gates.sh

# Real dataset random scenario benchmark (reproducible with seed)
bash scripts/contextset_random_benchmark.sh \
  --dataset /Users/axient/Documents/contextSet \
  --sample-size 24 \
  --seed 4242 \
  --report-path docs/REAL_CONTEXTSET_VALIDATION_$(date +%F)-random.md

# Real dataset matrix benchmark (recommended release gate)
bash scripts/contextset_random_benchmark_matrix.sh \
  --dataset /Users/axient/Documents/contextSet \
  --sample-size 24 \
  --seeds 4242,777,9001 \
  --report-path docs/REAL_CONTEXTSET_VALIDATION_MATRIX_$(date +%F).md
# default gate: non-empty + top1 + top5 thresholds with per-seed p95 and reason columns
# heading candidates exclude YAML front matter and fenced code blocks

# iOS simulator test (example)
xcodebuild test \
  -project /Users/axient/repository/AxiomMe-ios-app/AxiomMeIOSApp.xcodeproj \
  -scheme AxiomMeIOSApp \
  -destination 'platform=iOS Simulator,id=<SIMULATOR_ID>'

# iOS device test (example)
xcodebuild test \
  -project /Users/axient/repository/AxiomMe-ios-app/AxiomMeIOSApp.xcodeproj \
  -scheme AxiomMeIOSApp \
  -destination 'id=<DEVICE_UDID>' \
  -allowProvisioningUpdates
```

## Release Gate Addendum

For mobile-native readiness, "tests pass" is not enough. The gate must include:

1. zero unresolved deployment-target mismatch warnings,
2. bounded concurrency proof on high-fanout paths,
3. UI-path confirmation that all heavy work is off-main,
4. ABI compatibility checks for FFI payload schema and ownership.
