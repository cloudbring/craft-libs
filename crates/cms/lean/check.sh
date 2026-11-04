#!/usr/bin/env bash
# Verify the craft-cms sRGB Lean development end to end:
#   1. build the proofs, 2. audit axioms, 3. regenerate vectors and compare to the committed CSV.
# Run from anywhere; requires elan on PATH (or in ~/.elan/bin).
set -euo pipefail

export PATH="$HOME/.elan/bin:$PATH"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
csv="$here/../tests/vectors/srgb.csv"
cd "$here"

command -v lake >/dev/null || { echo "FAIL: lake not found; install elan" >&2; exit 2; }
start=$SECONDS

echo "== lake exe cache get =="
if grep -q 'mathlib' lakefile.toml; then
  lake exe cache get
fi

echo "== lake build =="
lake build

echo "== axiom audit =="
audit_out="$(mktemp)"; new_csv="$(mktemp)"
trap 'rm -f "$audit_out" "$new_csv"' EXIT
lake env lean Audit.lean > "$audit_out" 2>&1 || { cat "$audit_out"; echo "FAIL: Audit.lean did not elaborate" >&2; exit 1; }
cat "$audit_out"

expected=$(grep -c '^#print axioms ' Audit.lean)
# Flatten whitespace so axiom lists wrapped across lines parse as one record, then
# split into one record per "'name' ..." entry.
records="$(tr '\n' ' ' < "$audit_out" | sed -E "s/ +/ /g; s/ ('[^']+' (depends on axioms|does not depend))/\n\1/g" | sed '/^ *$/d')"
seen=$(printf '%s\n' "$records" | grep -c "^'" || true)
if [ "$seen" -ne "$expected" ]; then
  echo "FAIL: audit printed $seen records, Audit.lean requests $expected" >&2; exit 1
fi
bad=0
while IFS= read -r rec; do
  case "$rec" in
    *"does not depend on any axioms"*) continue ;;
    *"depends on axioms: ["*)
      list="${rec#*axioms: \[}"; list="${list%%\]*}"
      IFS=',' read -r -a axs <<< "$list"
      for a in "${axs[@]}"; do
        a="$(echo "$a" | tr -d ' ')"
        case "$a" in
          propext|Classical.choice|Quot.sound) ;;
          *) echo "FAIL: disallowed axiom '$a' in: $rec" >&2; bad=1 ;;
        esac
      done ;;
    *) echo "FAIL: unparseable audit record: $rec" >&2; bad=1 ;;
  esac
done <<< "$records"
# Belt and braces: sorryAx anywhere in output, including warnings about sorry.
if grep -Eq 'sorryAx|declaration uses .sorry.' "$audit_out"; then echo "FAIL: sorry detected" >&2; bad=1; fi
[ "$bad" -eq 0 ] || exit 1
echo "audit OK: $seen theorems, only propext/Classical.choice/Quot.sound"

echo "== regenerate vectors =="
lake env lean Generate.lean > "$new_csv"
if cmp "$csv" "$new_csv"; then
  echo "vectors OK: $(($(wc -l < "$csv") - 1)) rows byte-identical to tests/vectors/srgb.csv"
else
  echo "FAIL: tests/vectors/srgb.csv differs from Lean-generated output" >&2
  diff "$csv" "$new_csv" | head -10 >&2 || true
  exit 1
fi
echo "check.sh OK in $((SECONDS - start))s"
