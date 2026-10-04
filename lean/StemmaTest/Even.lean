import Stemma

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Even numbers" =>

A natural number is *even* if it is twice another.

:::definition (label := "even") (central := true)
A natural number $`n` is *even* if $`n = 2k` for some natural $`k`.

```lean
def IsEven (n : Nat) : Prop := ∃ k, n = 2 * k
```
:::

Lean outside an environment is machinery:

```lean
theorem two_mul_eq (k : Nat) : 2 * k = k + k := by omega
```

:::theorem (label := "even-add") (central := true) (title := "Sum of even numbers")
The sum of two even numbers is even.

```lean
theorem IsEven.add {a b : Nat} (ha : IsEven a) (hb : IsEven b) : IsEven (a + b) := by
  obtain ⟨k, rfl⟩ := ha
  obtain ⟨l, rfl⟩ := hb
  exact ⟨k + l, by omega⟩
```
:::

:::proof (of := "even-add")
Write $`a = 2k` and $`b = 2l`; then $`a + b = 2(k + l)`.
:::

:::lemma (label := "even-mul") (central := false)
The product of an even number by any number is even.

```lean
theorem IsEven.mul {a : Nat} (b : Nat) (ha : IsEven a) : IsEven (a * b) := by
  sorry
```
:::

:::conjecture (label := "even-open") (central := false)
A statement nobody has formalized yet.
:::

:::example
Four is even: {lean}`IsEven 4`.
:::
