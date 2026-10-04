# The Stemma specification

Status: draft.

A Stemma library is the Lean library a research group builds together. It is
the main body of the group's mathematical work: definitions, results, proofs
and the prose that explains them. Papers are produced from it. People work by
talking to an agent; the agent edits the library.

This document says what a Stemma library is. The `stemma` command line
implements it, and the remote enforces it.

## 1. Layout

A Stemma library is a git repository with this layout. `<Library>` is the
library's Lean name, for example `Algebra`.

```
<library>/
├── stemma.toml            # the library's configuration
├── lakefile.toml          # Lake package; requires `stemma` (and Mathlib)
├── lake-manifest.json
├── lean-toolchain
├── <Library>.lean         # the root module: the table of contents
├── <Library>/             # the library itself, organized by subject
│   └── …/<Module>.lean
├── references.bib         # the bibliography
├── signatures/            # human signatures, one file per signed result
│   └── <label>.toml
├── readings/              # curated readings of the library: guides, papers
│   └── <reading>/
├── AGENTS.md              # points agents at `stemma`
├── README.md
└── .gitignore
```

Who writes each entry:

| Entry | Written by |
|---|---|
| `stemma.toml` | `stemma`, on request |
| `lakefile.toml`, `lake-manifest.json`, `lean-toolchain` | `stemma` only |
| `<Library>.lean` | agents, and people who want to |
| `<Library>/` | agents, and people who want to |
| `references.bib` | agents; every entry checked by a person |
| `signatures/` | the signing step only, never an agent |
| `readings/` | agents, and people who want to |
| `AGENTS.md`, `.gitignore` | `stemma` only |

### Organization is free; every module reads on its own

- Modules under `<Library>/` are organized by subject, as deep as needed, the
  way Mathlib is. The specification fixes no hierarchy and no names beyond
  Lean's: a module `<Library>/A/B.lean` is `<Library>.A.B`.
- **Every module is readable by itself**, and the prose that explains a
  declaration sits beside it, so that the two change together (§2).
- How large a module is, and where it is split, is decided by the mathematics
  and by Lean (dependencies, elaboration time), not by a narrative.

### The root module is the table of contents

- `<Library>.lean` imports every module of the library, each exactly once.
  `stemma check` fails when a module is missing.
- The order of its imports is the order in which the site presents the
  modules. Lean ignores that order, so it is free for the reader. Nesting
  comes from module names: siblings appear in the order in which the first of
  them is imported.
- After its imports, the root module may be a document (§2) whose prose is the
  library's front page.
- Anyone who depends on the library writes `import <Library>`.

### Readings are views of the library

- The library's site presents the modules in the order of the table of
  contents.
- When a group wants to tell something in another order (an introduction to a
  subject, the road to a result), it writes a **reading**: a narrative that
  goes through modules and results in the order it chooses, and cites them.
- A paper is a reading whose destination is a journal rather than the site.
  Guides and papers are the same kind of thing, and both cite the library in
  the same way.

### Agent instructions are not copied into the library

The rules, skills and permissions agents work with belong to the version of
`stemma` the library uses, which loads them when it starts an agent
(`stemma claude`, `stemma codex`). The library only carries `AGENTS.md`, which
says to work through `stemma`, so that an agent started directly knows it is
not equipped.

### Dependencies

Every Stemma library depends on Lean, on `stemma`'s Lean package, on Verso
(through documents, §2) and usually on Mathlib. `stemma` alone chooses their
versions: each release of `stemma` names a combination of Lean, Verso and
Mathlib that it has checked works together (the packages they share pinned to
the same revisions), and `stemma` moves a library from one combination to
another as a single change.

### Not committed

`.lake/` (Lean builds and dependencies), the built site and other build
outputs, and `.stemma/`, the command line's local state (including the
program that builds the site).

## 2. Modules

A module of the library is one of two kinds:

- a **document**, where the mathematics is written: prose, mathematical
  environments and the Lean that formalizes them;
- a **Lean module**, an ordinary Lean file: machinery that needs no prose
  (technical APIs, instances, low-level lemmas). Everything in it is dark work:
  it has no environments and nothing in it is signed. The site shows it as
  code.

`stemma new <Module>` creates a module of either kind, with the standard
header, and adds it to the table of contents.

### Documents

A document is a [Verso](https://github.com/leanprover/verso) document in the
`Stemma` genre:

````
import Stemma
import Mathlib.Algebra.Group.Even

open Verso.Genre Manual InlineLean Stemma

#doc (Stemma) "Even numbers" =>

A natural number $`n` is *even* if it is twice another.

:::definition (label := "def:even") (central := true)
A natural number $`n` is *even* if $`n = 2k` for some natural $`k`.

```lean
def IsEven (n : Nat) : Prop := ∃ k, n = 2 * k
```
:::

Lean outside an environment is machinery:

```lean
theorem two_mul_add_zero (k : Nat) : 2 * k + 0 = 2 * k := rfl
```
````

- **Prose comes first.** After the header, the file is Verso markup:
  paragraphs, sections, mathematics (`` $`…` ``, `` $$`…` ``), lists, and
  references to Lean (`` {lean}`…` ``, `` {name}`…` ``), which Lean checks.
- **Lean goes in ` ```lean ` blocks.** They are elaborated in order, and what
  they declare is part of the library: a module that imports the document
  sees those declarations as it would see any others.
- **The header is the imports, the `open` line and `#doc`.**
- The `Stemma` genre decides how documents are numbered, cross-referenced and
  rendered. It starts as Verso's `Manual` genre under another name, and
  becomes its own as Stemma needs.

### Environments

A mathematical environment (`:::definition`, `:::theorem`, `:::proof`, …)
is the unit of the mathematical text, and the unit of state and signature.

- **The Lean inside an environment is its formalization.** Stemma records,
  for each environment, the declarations its ` ```lean ` blocks add. The
  relation between the text and its formalization is computed by the
  elaborator, not written by hand.
- **An environment without Lean is not formalized yet**, and is shown as such.
- **Lean outside every environment is dark work**: part of the library, but
  not part of the mathematical text, and never signed.

#### Base kinds

Every environment has a base kind, which decides how Stemma treats it:

| Base kind | What it is | Its Lean | What a signature covers |
|---|---|---|---|
| `definition` | Introduces a concept or a construction | `def`, `structure`, `class`, `instance`, … | The prose, and the types and values of its declarations with proofs erased, with their closure |
| `statement` | Claims something | `theorem` (`axiom` only when cited) | The prose, and the types of its declarations, with their closure |
| `proof` | The informal proof of a definition or a statement | Optional: auxiliary lemmas, which are dark work | Never signed |
| `remark` | A remark, an example, a note | Optional (an `example`, say) | Never signed, never central |

- The formal proof of a statement is the body of its `theorem`: in Lean, a
  statement and its proof are one declaration. A `proof` environment holds the
  informal proof.
- The **closure** of a declaration is everything its meaning depends on: the
  definitions its type (and, for a definition, its value) uses, transitively.
  Proofs are not part of it: by proof irrelevance, a different proof cannot
  change what is defined or established.

#### Names

Each environment has a name shown to readers, which maps to a base kind.
Stemma ships these:

| Name | Base kind |
|---|---|
| Definition, Construction | `definition` |
| Theorem, Lemma, Proposition, Corollary, Conjecture | `statement` |
| Proof | `proof` |
| Remark, Example, Note | `remark` |

A group registers its own names with a Lean command, in a module imported
before they are used (for example, a `hypothesis` named "Hypothesis" on top of
`statement`).

#### Marks

| Mark | On | Required | Meaning |
|---|---|---|---|
| `label` | `definition`, `statement` | yes | A stable identifier, unique in the library, that readings, papers and signatures cite. It cannot change once signed. |
| `label` | `proof`, `remark` | no | The same, when something needs to cite them. |
| `central` | `definition`, `statement` | yes, `true` or `false` | Whether the environment is central: signed and audited (§ signatures). |
| `of` | `proof` | yes | The label of the definition or statement it proves, which must already exist, in the same module or an imported one. |
| `title` | any | no | A name shown with the environment: "Theorem 3.2 (Zorn's lemma)". |
| `cited` | `definition`, `statement` | no | A reference in `references.bib`: the environment's obligations are accepted from the literature instead of proved. |

Dependencies ("this uses that definition") are not marks: they are computed
from the Lean.

#### Obligations and states

- The **obligations** of an environment are the proofs its formalization
  needs: for a statement, the proofs of its theorems; for a definition, the
  proof parts of its values (a construction must show that it has the
  structure it promises; a plain definition has no obligations). Lean tells
  them apart, so they are computed, never marked.
- The state of a definition or a statement is computed:
  - **not formalized**: it has no Lean;
  - **pending**: some obligation depends on `sorry`;
  - **cited**: some obligation rests on axioms allowed by `cited`;
  - **proved**: none of the above.

#### Rules

The elaborator enforces these; breaking one is a compilation error.

- An environment's kind and required marks are known and present.
- Labels are unique in the library.
- **Data never depends on `sorry` or on axioms.** `sorry` is allowed only in
  obligations: a `sorry` in the data would change what is defined.
- **An `axiom` is allowed only inside a `cited` environment, and only for a
  proposition**: an obligation accepted from the literature, never an object.
  So the axioms a result depends on say exactly which literature it rests on.
- A `proof` names, with `of`, an existing definition or statement.

## Open questions

1. **Signatures.** One file per signed result avoids merge conflicts; one lock
   file is easier to read. How a signature is made, and by whom, is part of
   the signature protocol (to be written).
2. **Private work.** Whether a person can keep modules private within the
   group, and where they live.
3. **Readings** (`readings/`). Their format, how they cite the
   library, and whether a paper lives in the library repository or in its
   own, citing a release of the library.
4. **`stemma.toml`.** What it holds: the library's title, the pinned `stemma`
   version, the group's members and who can sign.
5. **References across libraries.** How a document cites an environment of
   another Stemma library it depends on.
