#!/usr/bin/env bash
# The report `stemma-extract` gives of the test library contains what it must.
set -uo pipefail
cd "$(dirname "$0")/.."
report=$(lake exe stemma-extract StemmaTest) || { echo "FAIL stemma-extract exited with an error"; exit 1; }
status=0
while IFS= read -r expected; do
  if grep -qF -- "$expected" <<<"$report"; then
    echo "ok   $expected"
  else
    echo "FAIL missing: $expected"; status=1
  fi
done <<'EXPECTED'
{"kind":"document","name":"StemmaTest.Even"}
{"kind":"lean","name":"StemmaTest.Machinery"}
"label":"even-add"
"state":"cited"
"state":"notFormalized"
"formal":"sha256:
The module StemmaTest.Machinery is missing from the table of contents
The label 'twin' is used twice
The axiom 'stray' is outside every environment
EXPECTED
exit $status
