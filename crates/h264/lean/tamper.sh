#!/usr/bin/env bash
# Tamper tests: check.sh must FAIL when (1) a vector digit is flipped, (2) a `sorry` theorem is audited,
# (3) a non-SAT theorem depends on Lean.ofReduceBool (native_decide). Files are restored on exit.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
csv="$here/../tests/vectors/lean-idct.csv"
tight="$here/CraftIDCT/Tight.lean"; audit="$here/Audit.lean"
bcsv="$(mktemp)"; btight="$(mktemp)"; baudit="$(mktemp)"
cp "$csv" "$bcsv"; cp "$tight" "$btight"; cp "$audit" "$baudit"
restore() { cp "$bcsv" "$csv"; cp "$btight" "$tight"; cp "$baudit" "$audit"; rm -f "$bcsv" "$btight" "$baudit"; }
trap restore EXIT
expect_fail() {
  if "$here/check.sh" > /dev/null 2>&1; then echo "FAIL: check.sh accepted: $1" >&2; exit 1; fi
  echo "tamper OK: check.sh rejected: $1"
}
# (2) sorry
sed -i.tmp '$d' "$tight"; rm -f "$tight.tmp"   # drop trailing `end CraftIDCT`
printf 'theorem tamper_sorry : (1 : Nat) = 2 := by sorry\nend CraftIDCT\n' >> "$tight"
echo '#print axioms CraftIDCT.tamper_sorry' >> "$audit"
expect_fail "a theorem using sorry"
# (3) native_decide in a non-SAT theorem
cp "$btight" "$tight"; cp "$baudit" "$audit"
sed -i.tmp '$d' "$tight"; rm -f "$tight.tmp"
printf 'theorem tamper_native : (2 : Nat) + 2 = 4 := by native_decide\nend CraftIDCT\n' >> "$tight"
echo '#print axioms CraftIDCT.tamper_native' >> "$audit"
expect_fail "native_decide outside the SAT theorems"
# (1) vector digit flip (last digit of the first 4x4 random row's residual)
cp "$btight" "$tight"; cp "$baudit" "$audit"
python3 - "$csv" <<'PY'
import sys
p = sys.argv[1]
lines = open(p).read().split('\n')
i = next(k for k, l in enumerate(lines) if l.startswith('4,random_0,'))
f = lines[i].split(',')
f[4] = f[4][:-1] + ('1' if f[4][-1] != '1' else '2')
lines[i] = ','.join(f)
open(p, 'w').write('\n'.join(lines))
PY
cmp -s "$csv" "$bcsv" && { echo "tamper did not change the CSV" >&2; exit 2; }
expect_fail "a tampered vector CSV"
