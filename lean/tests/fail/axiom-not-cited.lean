-- expect: allowed only in an environment marked 'cited'
import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Failure" =>

:::theorem (label := "assumed") (central := false)
Assumed without a citation.

```lean
axiom assumed : 1 = 1
```
:::
