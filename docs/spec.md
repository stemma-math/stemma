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
├── <Library>.lean         # the root module, generated: imports every module
├── <Library>/             # the library itself, organized by subject
│   └── …/<Module>.lean
├── references.bib         # the bibliography
├── signatures/            # human signatures, one file per signed result
│   └── <label>.toml
├── documents/             # curated readings of the library: guides, papers
│   └── <document>/
├── AGENTS.md              # points agents at `stemma`
├── README.md
└── .gitignore
```

Who writes each entry:

| Entry | Written by |
|---|---|
| `stemma.toml` | `stemma`, on request |
| `lakefile.toml`, `lake-manifest.json`, `lean-toolchain` | `stemma` only |
| `<Library>.lean` | `stemma` only |
| `<Library>/` | agents, and people who want to |
| `references.bib` | agents; every entry checked by a person |
| `signatures/` | the signing step only, never an agent |
| `documents/` | agents, and people who want to |
| `AGENTS.md`, `.gitignore` | `stemma` only |

### Organization is free; every module reads on its own

- Modules under `<Library>/` are organized by subject, as deep as needed, the
  way Mathlib is. The specification fixes no hierarchy and no names beyond
  Lean's: a module `<Library>/A/B.lean` is `<Library>.A.B`.
- **Every module is readable by itself.** It opens with a module docstring
  that gives its title and introduces it, and its prose and declarations follow
  in reading order. The prose that explains a declaration sits beside it, so
  that the two change together.
- How large a module is, and where it is split, is decided by the mathematics
  and by Lean (dependencies, elaboration time), not by a narrative.

### The root module is generated

`<Library>.lean` imports every module of the library, sorted, and nothing
else. `stemma` writes it, and `stemma check` fails when it is out of date. So
work in parallel never conflicts on it, and anyone who depends on the library
writes `import <Library>`.

### Readings are views of the library

- By default, the library's site presents the module tree: it always exists
  and needs no maintenance.
- When a group wants to tell something in order (an introduction to a
  subject, the road to a result), it writes a **document**: a narrative that
  goes through modules and results in the order it chooses, and cites them.
- A paper is a document whose destination is a journal rather than the site.
  Guides and papers are the same kind of thing, and both cite the library in
  the same way.

### Agent instructions are not copied into the library

The rules, skills and permissions agents work with belong to the version of
`stemma` the library uses, which loads them when it starts an agent
(`stemma claude`, `stemma codex`). The library only carries `AGENTS.md`, which
says to work through `stemma`, so that an agent started directly knows it is
not equipped.

### Not committed

`.lake/` (Lean builds and dependencies), the built site and other build
outputs, and `.stemma/`, the command line's local state.

## Open questions

1. **Signatures.** One file per signed result avoids merge conflicts; one lock
   file is easier to read. How a signature is made, and by whom, is part of
   the signature protocol (to be written).
2. **Private work.** Whether a person can keep modules private within the
   group, and where they live.
3. **Documents.** Their format (Lean with prose, Verso, LaTeX for papers), how
   they cite the library, and whether a paper lives in the library repository
   or in its own, citing a release of the library.
4. **`stemma.toml`.** What it holds: the library's title, the pinned `stemma`
   version, the group's members and who can sign.
