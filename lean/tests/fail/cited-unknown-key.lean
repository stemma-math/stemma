-- expect: The reference 'Nowhere2000' is not in references.bib
import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Failure" =>

:::theorem (label := "from-nowhere") (central := false) (cited := "Nowhere2000, Thm. 1")
A result cited from a reference the library does not have.
:::
