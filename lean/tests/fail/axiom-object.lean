-- expect: is not a proposition
import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Failure" =>

:::definition (label := "postulated") (central := false) (cited := "Nobody")
An object postulated, not defined.

```lean
axiom postulated : Nat
```
:::
