---
name: stemma-material
description: How to work from the group's material in a Stemma library — references in references.bib, each person's files in sources/, and the shared notes and drafts in scratch/ — building the library the mathematics calls for rather than copying a document. Use when the person points at a book, paper, PDF, LaTeX or Typst file, notes or a draft, wants a reference formalized, or has changed something in scratch/.
---

# Working from material

You are building a mathematical library, not transcribing a document. Books,
papers, notes and drafts feed the library; they do not give it its shape.
Modules, names, order and generality are decided by the mathematics, as
`stemma-mathematics` says, whatever the structure of the material.

## Where material lives

| | In git | Citable | What it is |
|---|---|---|---|
| `references.bib` | yes | yes, with a place in the work | The fixed works the library relies on |
| `sources/` | no | through its entry in `references.bib` | Each person's copies: PDFs, also of books that cannot be shared |
| `scratch/` | yes | no | The group's living material: notes, drafts, intentions |

- **`sources/` is ignored by git**, and many search tools skip ignored files.
  List and read it directly (`ls sources/`, then open the file) instead of
  relying on a search. What is in it differs from person to person: if a
  reference you need is not there, ask the person for it rather than guess
  its content.
- **`scratch/`** has a recommended layout, which the group may not follow:
  `scratch/share/` for the group, and `scratch/<member>/` for each member,
  with a `main.md`, `main.tex` or `main.typ`. Look at what is there.

## Citing

- Cite only entries of `references.bib`: with the `cite` role in prose, or
  with a `cited` mark on an environment whose obligations are accepted from
  the literature (`stemma-documents`).
- A place in the work (`{cite "Thm. 3.24"}`, `(cited := "Key, Thm. 3.24")`)
  belongs to a fixed reference, and is worth giving when it helps a reader:
  attributing a known result, pointing to a proof not formalized.
- Material in `scratch/` is not cited, not even by its numbering ("Lemma 2
  of the notes"). If the person wants a draft cited, propose adding it to
  `references.bib`; a person checks every entry.

## Living material: `scratch/`

What is in `scratch/` is the group thinking: first ideas of what to build,
definitions being tried, directions that change. Treat it as the person's
voice, not as a specification.

- **Take the ideas, not the form.** Decide modules and names by the
  mathematics; do not make a module per section or a declaration per
  numbered item, and do not carry the draft's numbering into the library.
- **Improve on it.** When the library is better served by another
  definition, a more general statement or a different route, propose it,
  with a recommendation, as `stemma-mathematics` says.
- **Follow its changes.** At the start of a session, and when the person
  says they changed something, look at what changed (`git log -p --
  scratch/`, or `git diff` for what is not committed) and say briefly what
  you understand they now want before acting on it.
- **Writing in it.** You may write in `scratch/` when it helps the person:
  an inventory, a proposal, open questions, in `scratch/share/` or in a file
  of your own. Do not rewrite what a person wrote there; add beside it.
- What the library already holds may be removed from `scratch/` when the
  person wants: the history keeps it.

## Formalizing a reference

When the person asks to formalize a fixed reference (a book, a paper), the
library still has its own structure; the reference is cited, not mirrored.

1. **Inventory.** Read it and list what it has (definitions, numbered
   results, notation, standing hypotheses, results quoted from elsewhere),
   with what each depends on. Keep the inventory in `scratch/` if the person
   wants to follow it there, in whatever form is useful; there is no fixed
   format.
2. **Scope and reuse, once.** Before writing any Lean, agree with the person
   which parts and what in them, what to take from Mathlib and from the
   library rather than define again (search first, as `stemma-mathematics`
   says), and how the reference's conventions and standing hypotheses become
   Lean. Write the answers down in `scratch/`.
3. **Work in batches.** Within that scope, keep going: take the next items in
   dependency order, write statements first, run `stemma check --json`,
   commit, and continue, without asking to continue after each batch. Stop
   and ask for what changes mathematical meaning or blocks the work: a
   definition that differs from Mathlib's, a generalization, a hypothesis to
   add, an ambiguity or an error in the reference, a dependency outside the
   scope. Gather routine choices into the report of each batch: what was
   done, what is pending or blocked and why, the choices made, what comes
   next.

## Honest progress

An environment whose obligations are pending (`sorry`) or rest on `cited`
axioms is stated, not proved. When you say how much is done, use the states
`stemma status --json` computes, and say what is proved and what is only
stated.

## Centrality and signing stay with people

Formalizing material does not make its results central. Propose candidates
(the main theorems, the definitions everything rests on), as
`stemma-mathematics` says, and leave signing to the people
(`stemma-signatures`).
