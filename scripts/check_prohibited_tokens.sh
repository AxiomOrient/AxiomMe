#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

legacy_word="$(printf '%b' '\x76\x69\x6b\x69\x6e\x67')"
legacy_prefix="$(printf '%b' '\x6f\x70\x65\x6e')${legacy_word}"
legacy_scheme="${legacy_word}://"
pattern="(${legacy_prefix}|${legacy_scheme}|\\b${legacy_word}\\b)"

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
    echo "prohibited legacy token detected"
    exit 1
fi

echo "prohibited-token scan passed"
