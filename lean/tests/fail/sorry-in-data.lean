-- expect: depends on 'sorry'
import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Failure" =>

:::definition (label := "half-done") (central := false)
A definition whose data is unfinished.

```lean
def halfDone : Nat := sorry
```
:::
