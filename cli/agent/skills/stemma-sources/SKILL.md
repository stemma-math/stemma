---
name: stemma-sources
description: How to formalize an existing source (a book, a paper, lecture notes, as PDF or LaTeX) faithfully in a Stemma library, alone or as a group, with a versioned plan in sources/. Use when the person wants to formalize a source, continue or report on its formalization, or set up or change its plan.
---

# Formalizing a source

When the goal is to formalize a given source rather than to do research, the
work follows the source, not the conversation: agree its scope once, then work
through it in batches without asking for "continue" after each one.

## 1. Inventory

Read the source (the PDF or the LaTeX the person gives) and list, part by part
(chapter, section), every item it has: definitions, numbered results, proofs,
notation, remarks, examples and exercises, with where they are ("Def. II.1.3",
"Thm. 4.2", "p. 37") and what each depends on, including notation and
conventions introduced in passing. Note what the source leaves implicit:
standing hypotheses, notation fixed at the start of a chapter, results it
quotes from elsewhere.

## 2. Scope and reuse, set once

Before writing any Lean, ask the person, once, and record the answers in the
plan (`sources/<source>/source.toml`):

- **the scope**: which parts, and what in them (every numbered definition and
  result; proofs or only statements; examples and exercises or not);
- **the reuse policy**: what to take from Mathlib, and from the library,
  rather than define again, and when a source's definition that differs from
  Mathlib's gets its own (say how they differ, and recommend);
- **conventions**: names, notation, how the source's standing hypotheses
  become Lean.

Search Mathlib and the library first, as `stemma-mathematics` says, so that
the reuse policy is decided on facts.

## 3. The plan

The plan lives in the library, so that the group shares one view of it:

```
sources/<source>/source.toml     # what the source is, its scope, its phase
sources/<source>/<part>.toml     # one file per part: its items
```

```toml
# sources/burris/source.toml
title = "A Course in Universal Algebra"
cite = "BurrisSankappanavar"      # its key in references.bib
phase = "planning"                # planning, initial, ended
scope = "Chapters I and II: every numbered definition and result, with proofs; no exercises."
reuse = "Mathlib's lattices and order theory; the library's own algebras."
conventions = "Operations are `Fin n → α → α`; names follow the source's numbering."
```

```toml
# sources/burris/ch2.toml
title = "II. The elements of universal algebra"
owner = "alice"                   # the part's owner: the default of its items

[[item]]
ref = "Def. II.1.1"               # where in the source; unique in the source
kind = "definition"               # definition, statement, proof, remark, notation, example, exercise
state = "done"                    # planned, in-progress, done, blocked, excluded
labels = ["algebra"]              # the environments that cover it
depends = []                      # refs of items of the same source

[[item]]
ref = "Thm. II.3.5"
kind = "statement"
state = "blocked"
reason = "Needs the correspondence theorem, Thm. I.4.12, which is out of scope."
owner = "bob"
depends = ["Def. II.1.1"]
```

- `done` means formalized: the item's environments exist and are named in
  `labels`. `blocked` and `excluded` say why in `reason`. An item `in-progress`
  has an owner. Owners are members of the library (`stemma.toml`).
- Keep one file per part, so that people working on different parts never
  touch the same file. Update the plan in the same commits as the work.
- `stemma check --json` validates the plan: known states, kinds and phases,
  labels that exist, owners who are members, dependencies that exist and form
  no cycle. `stemma status --json` gives its coverage (`sources`), by part and
  by owner.

## 4. Work in batches

Within the agreed scope, keep going: take the next items in dependency order,
write them (statements first, as `stemma-mathematics` says), run
`stemma check --json`, commit, update the plan, and continue. Do not stop to
ask for "continue" after each batch.

Stop and ask only for what changes mathematical meaning or blocks the work:

- a decision that changes what a statement says (a definition that differs
  from Mathlib's, a generalization, a hypothesis to add);
- an ambiguity or an error in the source;
- a hypothesis the source uses without stating it;
- a dependency outside the scope, or one that is blocked.

Gather routine questions (a name, a minor choice of route, an example to
skip) into the batch report instead, with what you chose. After each batch,
report briefly: what was done, what is pending or blocked and why, the
routine choices made, and what comes next.

**Never present built results as proved coverage while `sorry`s or
assumptions remain.** An item whose environments are pending (`sorry`) or rest
on `cited` axioms is formalized as a statement, not proved: say so, and use
the `proved` count of `stemma status`, never the `done` count, when you say how
much is proved.

## 5. Centrality and signing stay with people

Formalizing a source does not make its results central. Before sharing, and
at the end of a part, propose candidates for central (the source's main
theorems, the definitions everything rests on), as `stemma-mathematics`
says; when the person already marks central environments, propose only strong
candidates. Signing is theirs (`stemma-signatures`).

## 6. As a group

When a group starts a library from a known base (a book it builds on, its own
papers), formalizing it is shared work:

1. **Agree the plan before work starts** (`phase = "planning"`): the
   inventory, the scope, the reuse policy, and the parts.
2. **Assign parts to members**: each part file names its `owner`, and items
   can name another.
3. **Fix shared conventions up front**, in `source.toml` and in the
   library's first modules: the core definitions everyone builds on, naming,
   what to reuse from Mathlib, and the environments meant to be central.
   Formalize those core definitions first, and share them, before the parts
   that use them.
4. **Work in parallel** (`phase = "initial"`): each member's agent works on
   its parts, marks items `in-progress` when it starts them, and shares often,
   so that others see the state in the plan on `main` rather than in every
   pull request.
5. **Declare the end of the initial phase** (`phase = "ended"`) when the
   group decides the base is done; what remains in the plan stays as a record
   of what was left out, and why.

Before starting an item, check its state and owner in the plan on `main`; do
not take an item someone else has in progress.
