import StemmaTest.Even
import StemmaTest.Environments
import StemmaTest.Machinery

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Constructions" =>

:::construction (label := "evens") (central := true)
The even numbers, as a subtype, are closed under addition.

```lean
structure Evens where
  val : Nat
  even : IsEven val

def Evens.add (x y : Evens) : Evens := ⟨x.val + y.val, by sorry⟩
```
:::

:::proof (of := "even-add")
A second proof, in another module, of a statement from the first.
:::

:::theorem (label := "big-even") (central := true) (cited := "Folklore")
A cited result: every number above a bound has an even neighbour.

```lean
axiom big_even : ∀ n : Nat, IsEven n ∨ IsEven (n + 1)
```
:::

:::hypothesis (label := "all-even") (central := false)
An environment this library registered itself.
:::
