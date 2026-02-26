# Mirror Field Migration Operations Report (2026-Q2)

Date: 2026-02-24  
Scope: `FindResult.memories/resources/skills` compatibility migration

## Objective

Track operational readiness for migrating consumers from compatibility mirrors to canonical fields:

- canonical fields: `query_results`, `hit_buckets`
- compatibility mirrors: `memories`, `resources`, `skills`

## Baseline Snapshot (2026-02-24)

1. Strict release-pack status:
   - `pack_id`: `d0d822ca-4813-405d-9183-0524fcba2e66`
   - `passed`: `true`
   - `unresolved_blockers`: `0`
   - `G0..G8`: `pass` (9/9)
2. Canonical-field references (`query_results|hit_buckets`) in core contract surfaces:
   - count: `30`
   - scope: `crates/axiomme-core/src/models/search.rs`, `docs/API_CONTRACT.md`, `docs/RELEASE_NOTES_2026-02-24.md`
3. Compatibility mirror vector fields in `FindResult`:
   - count: `3` (`memories`, `resources`, `skills`)
4. Mirror derivation points from canonical buckets (known internal sites):
   - `crates/axiomme-core/src/retrieval/engine.rs`
   - `crates/axiomme-core/src/models/search.rs`
   - `crates/axiomme-core/src/client/search/backend_tests.rs`

## Operations Checklist

| Item | Status | Owner | Evidence |
| --- | --- | --- | --- |
| Explicit release note published for mirror-field transition | done | runtime/core | `docs/RELEASE_NOTES_2026-02-24.md` |
| Strict release-pack gate rerun after transition notice | done | release/ops | `docs/RELEASE_PACK_STRICT_NOTICE_2026-02-26.json` |
| Canonical-only ranking contract documented (`query_results + hit_buckets`) | done | runtime/core | `docs/API_CONTRACT.md` section `FindResult` |
| External consumer inventory and parser migration progress (`query_results/hit_buckets` adoption rate) | done (local + non-local discovery) | integrator/ops | sections `External Consumer Inventory (Local Snapshot)` + `Non-local Inventory Refresh (2026-02-24)` |
| One-cycle advance notice window completed before mirror removal | done (post-notice release cycle observed) | release/ops | `docs/MIRROR_NOTICE_GATE_2026-02-24.json`; `docs/RELEASE_PACK_STRICT_NOTICE_2026-02-26.json`; section `One-cycle Notice Gate (2026-02-24 baseline)` in this report |

## External Consumer Inventory (Local Snapshot)

Scan scope: `/Users/axient/repository` companion repos (`AxiomMe-web`, `AxiomMe-mobile`, `AxiomMe-ios`, `AxiomMe-ios-app`, `AxiomMe-ios-ffi-sample`)

| Consumer Repo | Find/Search payload parser detected | Canonical key refs (`query_results/hit_buckets`) | Mirror key refs (`memories/resources/skills`) | Migration Status | Evidence |
| --- | --- | --- | --- | --- | --- |
| `AxiomMe-web` | yes | 1 | 0 | migrated to canonical payload path | `src/ui/index.js:501` (`payload.query_results`) |
| `AxiomMe-mobile` | no | 0 | 0 | n/a (no parser integration in scanned sources) | repo-wide key scan returned no matches |
| `AxiomMe-ios` | no | 0 | 0 | n/a (no parser integration in scanned sources) | repo-wide key scan returned no matches |
| `AxiomMe-ios-app` | no | 0 | 0 | n/a (no parser integration in scanned sources) | repo-wide key scan returned no matches |
| `AxiomMe-ios-ffi-sample` | no | 0 | 0 | n/a (FFI sample does not parse Find/Search payload keys) | repo-wide key scan returned no matches |

Local migration ratio (parser-integrated consumers only):

1. canonical adoption: `1 / 1` (`100%`)
2. mirror-key dependency: `0 / 1` (`0%`)

Aggregated local scan summary (2026-02-24):

- `parser_repos=1`
- `mirror_repos=0`
- `scanned=5`

## Non-local Inventory Refresh (2026-02-24)

Discovery method:

1. GitHub repo-name search for companion candidates:
   - queries: `AxiomMe-web`, `AxiomMe-mobile`, `AxiomMe-ios`, `AxiomMe-ios-app`, `AxiomMe-ios-ffi-sample`
   - observed `total_count` results:
     - `AxiomMe-web`: `0`
     - `AxiomMe-mobile`: `0`
     - `AxiomMe-ios`: `0`
     - `AxiomMe-ios-app`: `0`
     - `AxiomMe-ios-ffi-sample`: `0`
2. Organization direct remote probe:
   - `https://github.com/AxiomOrient/AxiomMe.git` -> reachable
   - `https://github.com/AxiomOrient/AxiomMe-web.git` -> `repository not found`
   - `https://github.com/AxiomOrient/AxiomMe-mobile.git` -> `repository not found`
   - `https://github.com/AxiomOrient/AxiomMe-ios.git` -> `repository not found`
   - `https://github.com/AxiomOrient/AxiomMe-ios-app.git` -> `repository not found`
   - `https://github.com/AxiomOrient/AxiomMe-ios-ffi-sample.git` -> `repository not found`

Result summary:

- Non-local refresh executions: `1`
- Public non-local companion consumers discovered: `0`
- Non-local mirror-key dependency findings: `0` (no discoverable public companion repos)

## One-cycle Notice Gate (2026-02-24 baseline)

Gate purpose:

- enforce API contract rule: mirror-field removal requires one full release cycle after explicit notice.

Notice anchor:

1. notice date: `2026-02-24`
2. notice source: `docs/RELEASE_NOTES_2026-02-24.md`

Release tag evidence:

1. known release tags (sorted):
   - `0.1.0` (`2026-02-16`)
   - `0.1.1` (`2026-02-22`)
   - `0.1.2` (`2026-02-23`)
   - `0.1.3` (`2026-02-26`)
2. first release tag strictly after notice date (`> 2026-02-24`): `0.1.3`

Current verdict:

- `ready`: post-notice release cycle is observed (`0.1.3`) and strict release gate passed at that checkpoint.
- scope note: this `ready` verdict is based on local workspace execution evidence; final release publication requires remote tag + CI run confirmation.

Completion rule for mirror-removal readiness:

1. publish at least one release tag with tag date strictly after `2026-02-24`.
2. keep strict release-pack gate passing at that checkpoint.
3. update this section verdict to `ready` and link the post-notice tag + gate evidence.

Latest automated gate run:

1. command:
   - `bash scripts/mirror_notice_gate.sh --workspace-dir . --json-output docs/MIRROR_NOTICE_GATE_2026-02-24.json`
2. output snapshot:
   - `status`: `ready`
   - `reason`: `post_notice_tag_and_strict_gate_passed`
   - `post_notice_tag`: `0.1.3`
   - `strict_gate.executed`: `true`
   - `strict_gate.passed`: `true`
   - `strict_gate.report_path`: `docs/RELEASE_PACK_STRICT_NOTICE_2026-02-26.json`
3. quality-gate integration:
   - `scripts/quality_gates.sh` runs `scripts/mirror_notice_router_smoke.sh` to verify route mapping invariants before snapshot refresh.
   - `scripts/quality_gates.sh` runs `scripts/mirror_notice_gate.sh` and refreshes `docs/MIRROR_NOTICE_GATE_2026-02-24.json`.
   - `scripts/quality_gates.sh` runs `scripts/mirror_notice_router.sh` and refreshes `docs/MIRROR_NOTICE_ROUTER_2026-02-24.json`.
   - when `AXIOMME_QUALITY_ENFORCE_MIRROR_NOTICE=on`, quality gates fail until this gate returns `status=ready`.
   - `.github/workflows/quality-gates.yml` sets this env to `on` for tag pushes and uploads both gate/router snapshots in artifact (`mirror-notice-gate`).

Latest router snapshot (same gate baseline):

1. command:
   - `bash scripts/mirror_notice_router.sh --gate-json docs/MIRROR_NOTICE_GATE_2026-02-24.json --output docs/MIRROR_NOTICE_ROUTER_2026-02-24.json`
2. output snapshot:
   - `selected_for_next`: `NX-009`
   - `route_type`: `actionable`
   - `route_reason`: `ready_or_unknown`
3. operator meaning:
   - proceed with one-cycle readiness closure flow for actual notice-date gate.

Readiness path simulation (notice-date override for harness validation):

1. command:
   - `bash scripts/mirror_notice_gate.sh --notice-date 2026-02-22 --workspace-dir . --strict-gate-output docs/RELEASE_PACK_STRICT_NOTICE_SIM_2026-02-22.json --json-output docs/MIRROR_NOTICE_GATE_SIM_2026-02-22.json`
2. output snapshot:
   - `status`: `ready`
   - `reason`: `post_notice_tag_and_strict_gate_passed`
   - `post_notice_tag`: `0.1.2`
   - `strict_gate.passed`: `true`
3. note:
   - this simulation validates the readiness path mechanics only.
   - production gate verdict must still use notice date `2026-02-24`.

## Release Publication Checkpoint (Post-Ready)

1. required:
   - push tag `0.1.3` to `origin`
   - verify tag-push CI (`quality-gates`) success with `mirror-notice-gate` artifact
2. current status:
   - blocked
   - tag publish: done (`git push origin 0.1.3`)
   - remote tag lookup: done (`git ls-remote --tags origin 0.1.3`)
   - tag-push CI: failed (`Quality Gates` run `22436388999`, step `Run quality gates`, exit `101`)
   - artifact check: run artifact list empty (`total_count=0`) for this failed run
3. remediation status:
   - local fix applied for CI failure root cause:
     - `crates/axiomme-core/src/models/benchmark.rs`: replaced manual `Default` impl with derive + `#[default]` variant for `ReleaseSecurityAuditMode`
     - `scripts/check_prohibited_tokens.sh`: added `grep` fallback when `rg` is unavailable
   - local verification: `cargo clippy -p axiomme-core --all-targets -- -D warnings` passed, `bash scripts/quality_gates.sh` passed
4. next required closure:
   - publish a commit containing remediation changes
   - create/push a new post-notice tag (for example `0.1.4`)
   - confirm tag-push `quality-gates` CI success and `mirror-notice-gate` artifact availability

## Repeatable Evidence Commands

```bash
rg -n "\b(query_results|hit_buckets)\b" \
  crates/axiomme-core/src/models/search.rs \
  docs/API_CONTRACT.md \
  docs/RELEASE_NOTES_2026-02-24.md | wc -l

rg -n "pub (memories|resources|skills): Vec<ContextHit>" \
  crates/axiomme-core/src/models/search.rs | wc -l

jq -r '.pack_id, .passed, .unresolved_blockers' \
  docs/RELEASE_PACK_STRICT_2026-02-24.json

jq -r '[.decisions[] | select(.status=="pass")] | length' \
  docs/RELEASE_PACK_STRICT_2026-02-24.json

cd /Users/axient/repository/AxiomMe-web
rg -n "payload\.(query_results|hit_buckets|memories|resources|skills)" src

cd /Users/axient/repository/AxiomMe-mobile
rg -n "payload\.(query_results|hit_buckets|memories|resources|skills)|\"(query_results|hit_buckets|memories|resources|skills)\"|case\s+(queryResults|hitBuckets|memories|resources|skills)" . -g '*.{kt,kts,java,swift,js,ts,tsx,jsx}'

repos=(AxiomMe-web AxiomMe-mobile AxiomMe-ios AxiomMe-ios-app AxiomMe-ios-ffi-sample)
for r in "${repos[@]}"; do
  base="/Users/axient/repository/$r"
  p=$(rg -n "payload\.(query_results|hit_buckets)|\"(query_results|hit_buckets)\"\s*:|case\s+(queryResults|hitBuckets)\b" "$base" -g '*.{js,ts,tsx,jsx,kt,kts,java,swift,m,mm,h}' 2>/dev/null | wc -l | tr -d ' ')
  m=$(rg -n "payload\.(memories|resources|skills)|\"(memories|resources|skills)\"\s*:|case\s+(memories|resources|skills)\b" "$base" -g '*.{js,ts,tsx,jsx,kt,kts,java,swift,m,mm,h}' 2>/dev/null | wc -l | tr -d ' ')
  echo "$r parser_keys=$p mirror_keys=$m"
done

for repo in AxiomMe-web AxiomMe-mobile AxiomMe-ios AxiomMe-ios-app AxiomMe-ios-ffi-sample; do
  q="$repo"
  curl -fsSL "https://api.github.com/search/repositories?q=${q}&per_page=5" \
    | jq -r '.items[] | "\(.full_name) :: \(.html_url)"'
done

for repo in AxiomMe-web AxiomMe-mobile AxiomMe-ios AxiomMe-ios-app AxiomMe-ios-ffi-sample; do
  url="https://github.com/AxiomOrient/${repo}.git"
  printf "%s -> " "$url"
  if git ls-remote --heads "$url" >/dev/null 2>&1; then
    echo "ok"
  else
    echo "not-found-or-private"
  fi
done

cd /Users/axient/repository/AxiomMe
git for-each-ref --sort=creatordate \
  --format='%(refname:short) %(creatordate:short) %(objectname:short)' refs/tags

bash scripts/mirror_notice_gate.sh \
  --workspace-dir . \
  --json-output docs/MIRROR_NOTICE_GATE_2026-02-24.json

jq '.' docs/MIRROR_NOTICE_GATE_2026-02-24.json

bash scripts/mirror_notice_router_smoke.sh

bash scripts/mirror_notice_router.sh \
  --gate-json docs/MIRROR_NOTICE_GATE_2026-02-24.json \
  --output docs/MIRROR_NOTICE_ROUTER_2026-02-24.json

jq '.' docs/MIRROR_NOTICE_ROUTER_2026-02-24.json
```

## Update Rule

1. Update this report at least once per release cycle during 2026-Q2.
2. Do not mark mirror removal as ready unless checklist item 5 is `done` and item 4 has at least one non-local inventory refresh.
3. If any strict gate fails (`passed=false` or unresolved blockers > 0), open a blocker and freeze removal schedule.
