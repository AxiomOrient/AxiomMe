#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-.}"
CRATE_DIR="${ROOT_DIR}/crates"
STRUCT_THRESHOLD="${STRUCT_THRESHOLD:-12}"
TOP_FILES="${TOP_FILES:-25}"
TOP_STRUCTS="${TOP_STRUCTS:-40}"

if [[ ! -d "${CRATE_DIR}" ]]; then
  echo "error: crates directory not found: ${CRATE_DIR}" >&2
  exit 1
fi

echo "# Large Object Scan"
echo "- root: ${ROOT_DIR}"
echo "- struct_field_threshold: ${STRUCT_THRESHOLD}"
echo "- top_files: ${TOP_FILES}"
echo "- top_structs: ${TOP_STRUCTS}"

echo
echo "## Top Files By LOC"
find "${CRATE_DIR}" -name '*.rs' -type f -print0 \
  | xargs -0 wc -l \
  | sort -nr \
  | head -n "${TOP_FILES}"

echo
echo "## Structs At/Above Field Threshold"
find "${CRATE_DIR}" -name '*.rs' -type f -print0 \
  | xargs -0 perl -0777 -ne '
      while (/(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)[^\{;]*\{(.*?)\n\}/sg) {
        my $name = $1;
        my $body = $2;
        my $count = () = ($body =~ /^\s*(?:pub\s+)?[A-Za-z_][A-Za-z0-9_]*\s*:/mg);
        print "$count\t$name\t$ARGV\n";
      }
    ' \
  | awk -F '\t' -v threshold="${STRUCT_THRESHOLD}" '$1 >= threshold' \
  | sort -nr \
  | head -n "${TOP_STRUCTS}"
