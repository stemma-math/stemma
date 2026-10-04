-- expect: already used
import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Failure" =>

:::lemma (label := "twice") (central := false)
First.
:::

:::lemma (label := "twice") (central := false)
Second.
:::
