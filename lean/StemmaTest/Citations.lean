import StemmaTest.Even

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Citations and proofs" =>

Prose cites the literature: {cite}`BurrisSankappanavar`, or a place in it:
{cite "Def. II.1.3"}`BurrisSankappanavar`.

:::definition (label := "algebra") (central := true) (cited := "BurrisSankappanavar, Def. II.1.3")
An *algebra* is a set with finitary operations on it.
:::

:::proof (of := "algebra")
A proof that immediately follows what it proves does not name it.
:::

:::theorem (label := "algebra-trivial") (central := false)
Every set is the carrier of the algebra with no operations, as in {cite}`Folklore`.
:::

Prose between a statement and its proof.

:::proof (of := "algebra-trivial")
A proof after other prose names what it proves.
:::

:::proof (of := "even-add")
A proof of a statement of another module names it.
:::
