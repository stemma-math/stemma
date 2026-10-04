#!/usr/bin/env bash
# Every file in tests/fail must fail to elaborate, with the message its first
# line names (`-- expect: <text>`; an empty text accepts any error).
set -uo pipefail
cd "$(dirname "$0")/.."
status=0
for f in tests/fail/*.lean; do
  expected=$(head -1 "$f" | sed -n 's/^-- expect: //p')
  if out=$(lake env lean "$f" 2>&1); then
    echo "FAIL $f: elaborated without errors"; status=1
  elif ! grep -qF -- "$expected" <<<"$out"; then
    echo "FAIL $f: expected '$expected', got:"; echo "$out" | head -5; status=1
  else
    echo "ok   $f"
  fi
done
exit $status
