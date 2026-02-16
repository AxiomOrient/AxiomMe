#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

blocked_word="$(printf '%b' '\x76\x69\x6b\x69\x6e\x67')"
blocked_prefix="$(printf '%b' '\x6f\x70\x65\x6e')${blocked_word}"
blocked_scheme="${blocked_word}://"
pattern="(${blocked_prefix}|${blocked_scheme}|\\b${blocked_word}\\b)"

if rg -n -i \
    --hidden \
    --glob '!.git/**' \
    --glob '!target/**' \
    --glob '!.axiomme/**' \
    --glob '!logs/**' \
    --glob '!Cargo.lock' \
    "$pattern" \
    .
then
    echo "prohibited token detected"
    exit 1
fi

echo "prohibited-token scan passed"
