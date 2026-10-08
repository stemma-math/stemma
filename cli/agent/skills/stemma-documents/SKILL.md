---
name: stemma-documents
description: How to write Stemma documents — prose, mathematical environments (definition, theorem, lemma, proof, remark…), their marks and rules, and creating modules. Use before writing or editing any document of a Stemma library.
---

# Writing Stemma documents

## Creating a module

```sh
stemma new Groups.Lagrange --title "Lagrange's theorem"   # a document
stemma new Groups.Internal --lean                          # a Lean module
stemma new Intro --after Stemma                            # placed first
```

`stemma new` writes the header and adds the module to the table of contents,
after its siblings. To reorder the table of contents, move import lines in
`<Library>.lean`: their order is the reading order.

## A document

````
import Stemma
import Mathlib.Algebra.Group.Basic

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Lagrange's theorem" =>

Prose, as in a paper. Mathematics: $`a + b`, display: $$`\sum_i x_i`.
References to Lean, checked by Lean: {lean}`Subgroup.index H`, {name}`Nat.Prime`.

:::theorem (label := "lagrange") (central := true) (title := "Lagrange")
The order of a subgroup divides the order of the group.

```lean
theorem card_subgroup_dvd_card … := by
  …
```
:::

:::proof (of := "lagrange")
The cosets of the subgroup partition the group, and all have its size.
:::
````

- Add the imports the Lean needs (Mathlib modules) to the header. Keep the
  `open` line as it is.
- Lean goes in ` ```lean ` blocks. Lean inside an environment is its
  formalization. Lean outside every environment is machinery (dark work):
  helpers the reader does not need.
- Write prose before the Lean it explains, and keep them together: when the
  formalization changes route, change the prose to follow it.
- Prose can only refer, with `{name}` or `{lean}`, to what is already
  declared above it.

## Citing the literature

References live in `references.bib`, at the root of the library. Prose cites
an entry by its key, optionally saying where in the work:

```
As in {cite}`BurrisSankappanavar`, or more precisely
{cite "Def. II.1.3"}`BurrisSankappanavar`.
```

- The key must be in `references.bib`; an unknown key does not compile. To
  cite something new, add its entry first (author, title and year are
  required; add `journal`, `booktitle` or `publisher`, and `doi` or `url`
  when known), and tell the person: they check every entry.
- Never invent bibliographic data. If you are not sure of an entry, ask.

## Environments

| Names | Base kind | Required marks | Optional marks |
|---|---|---|---|
| `definition`, `construction` | definition | `label`, `central` | `title`, `cited` |
| `theorem`, `lemma` (`Stemma.lemma` with Mathlib), `proposition`, `corollary`, `conjecture` | statement | `label`, `central` | `title`, `cited` |
| `proof` | proof | `of` | `label`, `title` |
| `remark`, `example`, `note` | remark | — | `label`, `title` |

- **With Mathlib, write `:::Stemma.lemma`**, not `:::lemma`: Mathlib's `lemma`
  command makes the plain name ambiguous ("directive name `lemma` is
  ambiguous"). The other names do not clash; any name may be written
  qualified, `:::Stemma.theorem`.
- **`label`**: lowercase letters, digits, `-` and `.` (`lagrange`,
  `groups.index-mul`), unique in the library. It is how everything cites the
  environment. Never change the label of a signed environment.
- **`central`**: `true` or `false`, always explicit. Central environments are
  what the library claims; a person signs them. Write `central := false`
  unless the person decided otherwise, and propose candidates for central
  when you finish a block of work (see `stemma-mathematics`).
- **`of`**: the label of the definition or statement a proof proves, already
  declared (above, or in an imported module). Place a proof right after what
  it proves when you can: the site then heads it "Proof."; elsewhere it reads
  "Proof of Theorem 8.", with a link.
- **`cited`**: the key of an entry of `references.bib`, optionally followed by
  a comma and where in the work: `(cited := "BurrisSankappanavar, Thm. 4.2")`.
  The key must exist. The environment's obligations are accepted from the
  literature: only then may its Lean contain `axiom`s, and only for
  propositions.
- A group can add names in a Lean module imported by its documents:
  `register_environment hypothesis : statement "Hypothesis"`.

## Rules the compiler enforces

- Marks present and valid; labels unique.
- **Data never depends on `sorry` or on axioms**: a `def`'s value may contain
  `sorry` only inside proofs (proof fields, `by sorry` obligations).
- `axiom` only in a `cited` environment, and only for propositions.

## States

An environment with no Lean is *not formalized*. A definition or statement
whose proofs depend on `sorry` is *pending*; on cited axioms, *cited*;
otherwise *proved*. `stemma status --json` lists them.
