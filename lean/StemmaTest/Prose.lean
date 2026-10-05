import Stemma

open Verso.Genre Manual InlineLean Stemma
open Lean Elab Command

#doc (Stemma) "Source prose regression" =>

:::definition (label := "prose-lists") (central := false)
Before the lists.

- First condition.
  - Nested condition.
- Last condition.

1. Ordered first.
2. Ordered second.

After the lists.

```lean
def proseRegressionValue : Nat := 1
```
:::

```lean
run_cmd do
  let env ← getEnv
  let some r := findRecord? env "prose-lists"
    | throwError "Missing test environment"
  for expected in #[
      "- First condition.", "- Nested condition.",
      "- Last condition.", "1. Ordered first.", "2. Ordered second.",
      "Before the lists.", "After the lists."] do
    unless (r.prose.splitOn expected).length > 1 do
      throwError "Missing prose: {expected}"
  let codeParts := r.prose.splitOn "proseRegressionValue"
  unless codeParts.length == 1 do
    throwError "Lean code leaked into prose"
```
