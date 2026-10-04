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
├── .github/               # the forge's checks and rules (GitHub)
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
| `.github/`, `AGENTS.md`, `.gitignore` | `stemma` only |

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
| `central` | `definition`, `statement` | yes, `true` or `false` | Whether the environment is central: signed and audited (§3). |
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

## 3. Signatures

A signature is a person's approval of one state of a central environment. It
is how people stay in charge of what the library claims without reviewing
everything agents write.

### What a signature covers

A signature covers, for a central `definition` or `statement`:

- **its identity**: its label and base kind (not its name, which is
  presentation);
- **its marks with meaning**: `central` and `cited` (not `title`);
- **its prose**: the text of the environment, normalized so that spacing does
  not count. The prose of its `proof` is not covered, as the formal proof is
  not;
- **its formal meaning**: the types of its declarations, and for a definition
  their values with proofs erased, together with their closure. The closure
  includes every declaration used, central or not: changing a dark-work
  definition that a signed statement uses changes what the statement says.
  What comes from Mathlib or from other pinned libraries counts by its
  content, so a version change only affects the signatures whose closure it
  actually changes.

A signature records two fingerprints, one of the prose and one of the formal
meaning, so that when it no longer matches, Stemma can say why.

### States

Only central environments have a signature state. It is independent of their
proof state (§2), so a statement can be signed and still pending: its
statement is approved, and its proof is not finished.

| State | Meaning |
|---|---|
| **unsigned** | Central, and never signed (new, or newly central) |
| **signed** | Both current fingerprints match a signature |
| **stale** | Signed, but a fingerprint no longer matches |

- An environment that is not formalized cannot be signed: a signature checks
  that the prose and the Lean say the same thing. Its statement may still be
  pending (`theorem … := sorry`), but it must exist.
- A stale environment says why: its prose changed, its formal statement
  changed, or something in its closure changed, naming it.
- **Changes cascade.** When a definition changes, every signed environment
  whose closure contains it becomes stale. It is never signed again by
  inheritance, because that is exactly the case a person must look at; but
  the cause and its consequences are signed together, in one act.

### What needs a signature

The shared branch never holds a stale environment: a change that would leave
one stale is not integrated until it carries the new signatures (§4). These
changes need a signature:

- changing the prose or the formal meaning of a signed environment, including
  through its closure (a dark-work definition it uses, a new Mathlib version);
- removing a signed environment;
- making a signed environment not central;
- changing a signed environment's label or its `cited` mark.

Everything else goes in without one: new central environments (unsigned
until someone signs them), proofs of signed statements, dark work outside
every signed closure, prose outside environments, and environments that are
not central.

### Who signs

- Any member with the `signer` role may sign.
- The person who directed a change may sign it: a signature guarantees that a
  person checked the correspondence, not that a second person did. A group
  that wants a second pair of eyes says so in `stemma.toml`:

  ```toml
  [signatures]
  distinct_from_author = true   # the signer cannot be the pull request's author
  ```

- A signature records who signed and, when there was one, which agent proposed
  the change (from the commit trailers).

### How a signature is made

Signing must be a deliberate act of a person. Agents run on that person's
machine with their credentials, so the aim is not to stop a malicious agent,
but to make sure that neither an agent's shortcut nor a person's offhand
"sign it for me" can produce a signature.

- A signer runs `stemma sign` in their own terminal. It shows what awaits a
  signature, grouped by cause: for each environment, its prose, its Lean
  statement and what changed since its last signature.
  The signer confirms each group explicitly.
- `stemma sign` writes the files in `signatures/` and makes a commit signed
  with the signer's SSH or GPG key, which goes into the pull request.
- Three barriers keep agents out:
  1. `stemma sign` runs only in an interactive terminal, which agents' shell
     tools are not;
  2. the agents `stemma` starts are not allowed to run it;
  3. the pull request's checks accept a change to `signatures/` only in
     commits that touch nothing else and are signed with the key of a
     member with the `signer` role.
- Agents prepare signatures but never make them: when a pull request needs
  signatures, the agent tells the person which ones, and why.

### Read-backs

A read-back is a voluntary aid to auditing, not part of signing.

- It is a translation of an environment's Lean into prose, made blind: by a
  fresh agent session that `stemma` starts with only the formal side (the
  statement as Lean prints it, and the definitions in its closure), never the
  environment's prose. It also points out Mathlib conventions that change the
  meaning (truncated subtraction, `x / 0 = 0`, junk values).
- A person asks for read-backs whenever they want: of everything new, of one
  environment, of a whole module.
- They are read in a local web page that sets each environment's prose beside
  its read-back. The page only shows them: the person decides whether they
  agree, and acts through the conversation.
- Read-backs are kept in `.stemma/`. They are not committed, and no signature
  requires one.

## 4. Collaboration

A library lives in a repository on a forge, with one shared branch, `main`.
Changes reach `main` only through pull requests that pass the library's
checks.

### Working

- Each person works on a branch of their own, `work/<person>` by default,
  which `stemma` creates the first time. Starting an agent does not
  synchronize anything: the person works, and the agent commits and pushes to
  that branch, which keeps the work safe and visible to the group.
- A branch may mix several things: a person need not decide in advance what a
  piece of work will turn into.
- People who know git work as they like (several branches, bringing `main` in
  whenever they want). `stemma` only asks that what reaches `main` goes
  through a pull request.

### Sharing

- Sharing is the only moment that looks at `main`. The agent brings in what
  is new there, resolves conflicts (asking the person, in mathematical terms,
  when two changes disagree about content) and opens or updates a pull request
  from the working branch.
- `stemma status` says how far a branch is from `main`, as information. It
  never forces a synchronization.
- When part of a branch needs a signature and part does not, the agent offers
  to split it into two pull requests, so that the second waits without holding
  back the first. The person decides.
- Pull requests are merged without squashing, so that the working branch stays
  valid after its work reaches `main`, and the next pull request carries only
  what is new.

### Checks

| When | Who | What |
|---|---|---|
| While working | The agent, with `stemma check` | Everything, for quick feedback; it guarantees nothing |
| On every push | The forge, at once | Protected files and layout |
| On a pull request | Required checks | Everything, on `main` with the pull request applied: the build, the rules of this specification, and signatures. This is what guarantees |

A pull request that would leave a signed environment stale says which
signatures it needs, and is not merged until they are added to it.

### Group policy

Each group decides, in `stemma.toml`, who may merge which pull requests:

```toml
[members]
alice = { roles = ["maintainer", "signer"] }
bob   = { roles = ["signer"] }
carol = { roles = [] }

[policy]
# Who may merge their own pull requests once every check passes. For them,
# `stemma` turns on the forge's auto-merge when they share.
self_merge = ["maintainer", "signer"]

# Which kinds of change also need the approval of another member with one of
# these roles.
[policy.review]
new-central = ["signer"]
policy      = ["maintainer"]
```

- A pull request whose author is in `self_merge` is merged as soon as every
  check passes. Otherwise it waits for the approval of a member with one of
  those roles.
- `policy.review` names kinds of change, computed from the content of the
  pull request (for example `new-central`, a new central environment, or
  `dependencies`, a change of versions), and the roles whose approval they
  need.
- Signatures are not reviews: a signature is about an environment, a review
  about a pull request. A pull request may need both, or neither.
- **The policy is always read from `main`, never from the pull request**, so
  that a pull request cannot change the policy that judges it. A change to
  `stemma.toml` always needs a maintainer's approval (`policy`), and that
  cannot be turned off.
- By default every member is in `self_merge`, and only `policy` needs a
  review.

### Forges

The first forge Stemma supports is GitHub:

| Piece | On GitHub |
|---|---|
| `main` changes only through pull requests | A ruleset on `main`: pull requests required, no direct or forced pushes |
| Required checks | A GitHub Actions workflow written by `stemma` (`.github/workflows/stemma.yml`), marked as a required status check |
| Merging by itself | GitHub's auto-merge |
| Required reviews | GitHub approvals, whose authors the policy check matches against roles |
| The policy read from `main` | The policy job reads `stemma.toml` from the base commit |
| The library's site | GitHub Pages, published from `main` by the same workflow |

- The build compiles the pull request's code, so it runs without secrets. The
  policy job runs nothing from the pull request.
- Builds cache `.lake` between runs.
- `stemma` sets the repository up (rules, workflow, Pages) when it creates the
  library.

## Open questions

1. **Signatures.** The format of `signatures/<label>.toml`.
2. **Private work.** Whether a person can keep modules private within the
   group, and where they live.
3. **Readings** (`readings/`). Their format, how they cite the
   library, and whether a paper lives in the library repository or in its
   own, citing a release of the library.
4. **`stemma.toml`.** What it holds besides members and policy: the library's
   title, the pinned `stemma` version.
5. **References across libraries.** How a document cites an environment of
   another Stemma library it depends on.
