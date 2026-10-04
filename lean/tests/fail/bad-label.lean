-- expect: Invalid label
import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Failure" =>

:::lemma (label := "Bad:Label") (central := false)
A label with forbidden characters.
:::
