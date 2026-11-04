#!/usr/bin/env bash
# Tamper test: flip one digit in tests/vectors/srgb.csv and require check.sh to FAIL; restore after.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
csv="$here/../tests/vectors/srgb.csv"
bak="$(mktemp)"; cp "$csv" "$bak"
trap 'cp "$bak" "$csv"; rm -f "$bak"' EXIT
# change the last digit of the decode8 row for code 128
sed -i.tmp -E '/^decode8,128,/ s/[0-9]$/9/;/^decode8,128,/ s/99$/98/' "$csv"; rm -f "$csv.tmp"
cmp -s "$csv" "$bak" && { echo "tamper did not change the file" >&2; exit 2; }
if "$here/check.sh" > /dev/null 2>&1; then echo "FAIL: check.sh accepted a tampered CSV" >&2; exit 1; fi
echo "tamper OK: check.sh rejected the tampered CSV"
