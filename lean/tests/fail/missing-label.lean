-- expect: Named argument 'label'
import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Failure" =>

:::theorem (central := true)
No label.
:::
