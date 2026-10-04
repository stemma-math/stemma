import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Duplicate B" =>

:::remark (label := "twin")
Module B uses a label that another module, which it does not import, also uses.
:::
