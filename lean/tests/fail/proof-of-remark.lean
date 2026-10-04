-- expect: must name a definition or a statement
import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Failure" =>

:::remark (label := "just-a-remark")
A remark.
:::

:::proof (of := "just-a-remark")
Proving a remark.
:::
