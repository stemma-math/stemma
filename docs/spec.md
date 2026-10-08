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
├── readings/              # curated readings: guides, papers (reserved)
│   └── <reading>/
├── .github/               # the forge's checks and rules (GitHub)
├── AGENTS.md              # the group's instructions for agents
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
| `AGENTS.md` | the group, except a block at its top that `stemma` writes |
| `.github/`, `.gitignore` | `stemma` only |

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
- It has a fixed form, so that `stemma` reads it exactly as Lean does: it
  begins with its imports, one `import <Module>` per line (blank lines between
  them are allowed), and nothing else comes before its first command.
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
- `readings/` is reserved for them; their format is not specified yet.

### References

`references.bib` is the library's bibliography, in BibTeX, and the source of
truth of its references: people edit it, and check every entry an agent adds.
`stemma init` creates it empty, and `stemma upgrade` creates it where it is
missing.

- Stemma reads, for each entry, its key, its authors (or editors), title and
  year, which are required, and its venue or publisher, and a `url` or `doi`
  when it has one. `@comment` and `@preamble` are skipped; `@string`
  abbreviations are not supported.
- The file is read strictly: a malformed entry, a repeated key, a missing
  required field or text outside every entry (other than `%` comments) is an
  error that names the line, never a silent omission.
- It is read when a document that cites it is elaborated (§2). Lake does not
  track it, so a document already built is checked again only when it is
  rebuilt; the site, which reads it every time it is built, fails on any
  citation of a key it lacks.

### The site

The library's site presents the modules in the order of the table of contents,
and then two pages of its own:

- an **index of central environments**, by page, with their labels and
  signature states;
- the **bibliography**: every entry of `references.bib`, with the environments
  that cite it.

Every environment with a label is anchored by it, so that
`…/<page>/#<label>` links to it, and shows its label discreetly, with a
button that copies it. Central environments carry a mark, and their signature
state (§3) when the site has it: `stemma preview` writes it, computed from
`signatures/`, to `stemma-state.json` beside the pages. A site built without
that file shows the marks and no state.

### Agent instructions are not copied into the library

The rules, skills and permissions agents work with belong to the version of
`stemma` the library uses, which loads them when it starts an agent
(`stemma agent <harness>`). `AGENTS.md` holds the group's own
instructions, and a block that `stemma` keeps at its top so that an agent
started without `stemma` notices it (§5).

### Dependencies

Every Stemma library depends on Lean, on `stemma`'s Lean package, on Verso
(through documents, §2) and usually on Mathlib. `stemma` alone chooses their
versions: each release of `stemma` names a combination of Lean, Verso and
Mathlib that it has checked works together (the packages they share pinned to
the same revisions), and `stemma upgrade` moves a library from one combination
to another as a single change, applying the migrations of the versions in
between. `stemma check` fails when the library uses another version than the
command line's, and says which of the two to change.

### Configuration

`stemma.toml` names the library and its members:

```toml
[library]
name = "Algebra"            # the library's Lean name
title = "Algebra"           # the site's title
stemma = "0.3.0"            # the stemma version, which fixes Lean, Verso and Mathlib

[members]
alice = { roles = ["maintainer", "signer"], keys = ["ssh-ed25519 AAAA…"] }

[policy]
require_signed_central = true   # every central environment is signed (§3)

[policy.review]             # optional (§4)
```

- A member is who signs with one of their **keys**: the SSH public keys listed
  for them. Their name is how the library refers to them; it need not be a
  forge account.
- `stemma init` makes the person who creates the library its first member, as
  maintainer and signer, with the key git signs with, so that someone can
  approve the changes the policy reserves to maintainers.
- A member registers their key with `stemma key add` (§3, "Who signs").
- `require_signed_central` says whether every central environment must carry a
  current signature (§3). It is on unless the group turns it off: `stemma
  init` writes it, and so does `stemma upgrade` in libraries made before it
  existed. Like the rest of the policy, it is read from `main`.

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
- **Prose cites the literature** with the `cite` role and a key of
  `references.bib`, optionally with where in the work:
  `` {cite}`BurrisSankappanavar` ``,
  `` {cite "Def. II.1.3"}`BurrisSankappanavar` ``. A key that is not in
  `references.bib` is an error, the way an unknown name in `{name}` is. On the
  site, a citation shows the authors and year, and links to the entry in the
  bibliography.
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

A document that imports Mathlib writes a lemma as `:::Stemma.lemma`: Mathlib
declares a `lemma` command, which makes the plain `:::lemma` ambiguous (Verso
reports "directive name `lemma` is ambiguous", and `stemma check` names the
qualified form). Every name can be written qualified, `:::Stemma.<name>`; of
the names Stemma ships, only `lemma` clashes with Mathlib.

A group registers its own names with a Lean command, in a module imported
before they are used (for example, a `hypothesis` named "Hypothesis" on top of
`statement`).

#### Marks

| Mark | On | Required | Meaning |
|---|---|---|---|
| `label` | `definition`, `statement` | yes | A stable identifier, unique in the library, that readings, papers and signatures cite: lowercase letters, digits, `-` and `.` (`even-add`, `groups.lagrange`). It cannot change once signed. |
| `label` | `proof`, `remark` | no | The same, when something needs to cite them. |
| `central` | `definition`, `statement` | yes, `true` or `false` | Whether the environment is central: signed and audited (§3). |
| `of` | `proof` | yes | The label of the definition or statement it proves, which must already exist, in the same module or an imported one. |
| `title` | any | no | A name shown with the environment: "Theorem 3.2 (Zorn's lemma)". |
| `cited` | `definition`, `statement` | no | A reference in `references.bib`, as its key, optionally followed by a comma and where in the work (`"BurrisSankappanavar, Thm. 4.2"`): the environment's obligations are accepted from the literature instead of proved. The key must be in `references.bib`; the site shows the reference on the environment. |

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
- A key cited by prose or by a `cited` mark is in `references.bib`.

#### On the site

Definitions, statements and remarks are numbered in their module. A proof's
heading names what it proves, "*Proof of Theorem 8.*", with a link to it on its
own page, unless it immediately follows it, with nothing but blank space
between them: then it is "*Proof.*".

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

On the shared branch, **every central environment is signed**: central means
signed by a person. `stemma check` fails while a central environment is
unsigned or stale, saying which and why, so a change is not integrated until
it carries the signatures it needs (§4). This covers every central environment
of the branch, not only those the change adds: one that reached `main`
unsigned (before this rule, or while the group had turned it off) blocks every
change until someone signs it, and `stemma status` lists it. A group can turn
the rule off (`require_signed_central = false`, §1), in a change of the
policy; then new central environments enter unsigned, and only stale ones
block.

Whatever the policy, the shared branch never holds a stale environment. These
changes need a signature:

- changing the prose or the formal meaning of a signed environment, including
  through its closure (a dark-work definition it uses, a new Mathlib version);
- removing a signed environment;
- making a signed environment not central;
- changing a signed environment's label or its `cited` mark.

A signature that applies to no central environment any more (because the
environment was removed, made not central, or relabelled) must be withdrawn by
a signer, with `stemma sign`, in the same change.

Everything else goes in without one: proofs of signed statements, dark work
outside every signed closure, prose outside environments, and environments
that are not central. New central environments need one under the default
policy, as above.

### What a branch brings

`stemma sign` and `stemma share` tell what a branch brings from what others
left pending, deterministically, from git and the report alone (no text is
parsed, and `main` is not built):

- an environment **belongs to the branch** when the lines it spans (the report
  records where each environment starts and ends) intersect what the branch
  changes since it forked from `main` (`git diff main...`, up to the working
  tree, which is what the report is built from), or when its module is new on
  the branch. Renames count as a removal and an addition, so an environment
  moved to another module belongs to the branch that moved it; bringing `main`
  in moves the fork point, so what `main` brought does not;
- an environment is **new** on the branch when all its lines are;
- an environment pending that does not belong to the branch is labelled with
  who last touched it: the author of the most recent commit among those that
  wrote its lines (`git blame`), and the agent its `Agent:` trailer names.

### Who signs

- Any member with the `signer` role may sign, including the person who
  directed the change: a signature guarantees that a person checked the
  correspondence.
- A member signs with a key listed for them in `stemma.toml` on `main`. They
  register it with `stemma key add [--key <path>]`, which takes the key git
  signs with (`user.signingkey`) by default and adds it to their entry, on a
  branch, never on `main`. It is a change of the policy, so a maintainer whose
  key is already on `main` approves it (§4): nobody approves their own first
  key. A key dedicated to signing is recommended. `stemma status` and `stemma
  verify` warn about members with the `signer` or `maintainer` role and no
  key.
- A signature records the member who signed, and, when there was one, which
  agent proposed the change (from the commit trailers).

### How a signature is made

Signing must be a deliberate act of a person. Agents run on that person's
machine with their credentials, so the aim is not to stop a malicious agent,
but to make sure that neither an agent's shortcut nor a person's offhand
"sign it for me" can produce a signature.

- A signer runs `stemma sign` in their own terminal. It shows what awaits
  them in two sections: what this branch brings (§3, "What a branch brings"),
  and what others left pending, each with who last touched it. For each
  environment it shows its label and name, whether it is new or what changed
  since its last signature, its prose and its Lean statement (for a statement,
  without its proof), where it is, and a link to its read-back when there is
  one; when they are too long, only where to read them. The signer chooses
  item by item: this branch's are chosen at first, others' are not.
- `stemma sign <labels…>` shows only those environments, all chosen at first.
- The changes on the branch that need their approval (§4), and the signatures
  to withdraw, are shown apart from signatures, and confirmed explicitly.
- `stemma sign` writes the files in `signatures/` and records the approvals in
  one commit, signed with the person's key, and says what is still missing,
  label by label. One signing session makes one commit.
- When the signing key belongs to no member, `stemma sign` offers to register
  it (as `stemma key add` does) and stops: registering a key and signing are
  separate acts, and the key counts only once a maintainer has approved it on
  `main`.
- Three barriers keep agents out:
  1. `stemma sign` runs only in an interactive terminal, which agents' shell
     tools are not;
  2. the agents `stemma` starts are not allowed to run it;
  3. `stemma verify` accepts a change to `signatures/` only in commits that
     touch nothing else and are signed with the key of a member with the
     `signer` role, as listed in `stemma.toml` on `main`. Git checks the
     signature; no forge is involved. A merge changes only what differs from
     every one of its parents: one that only combines them (bringing `main`
     into a branch, or a forge's merge of a pull request) changes nothing,
     and one that resolves a conflict changes what it resolves.
- Agents prepare signatures but never make them: when a change needs
  signatures, the agent tells the person which ones, and why.

### Signature files

Each signed environment has one file, `signatures/<label>.toml`, holding its
current signature; earlier ones are in the repository's history.

```toml
label = "even-add"
kind = "statement"
central = true
# cited = "Key2001, Prop 3.4"

signer = "alice"                  # the member who signed
signed = 2026-10-04T18:20:00Z
agent = "claude-opus-5-5"         # the agent that proposed the change, if any

[fingerprints]
version = 1                       # the fingerprint algorithm
prose = "sha256:…"
formal = "sha256:…"

# One fingerprint per declaration in the closure, so that Stemma can say
# what changed without rebuilding the signed state.
[closure]
"IsEven" = "sha256:…"
"IsEven.add" = "sha256:…"
"Nat.mul" = "sha256:…"
```

- `signer` and `signed` are informative: the proof of who signed is the
  commit's signature, made with the member's key, and the checks require the
  two to agree.
- `version` names the fingerprint algorithm, so that changing it never makes
  signatures stale silently: `stemma` computes every version, and migrates
  signatures explicitly.

### Read-backs

A read-back is a voluntary aid to auditing, not part of signing.

- It is a translation of an environment's Lean into prose, made blind: by a
  fresh agent session that `stemma` starts with only the formal side (the
  statement as Lean prints it, and the definitions in its closure), never the
  environment's prose. It also points out Mathlib conventions that change the
  meaning (truncated subtraction, `x / 0 = 0`, junk values).
- A person asks for read-backs whenever they want: of everything new, of one
  environment, of a whole module.
- **They are for the person, who reads them first.** `stemma readback` makes
  them and serves a local page; the agent hands over its address. What the
  command says to agents (`--json`) is each read-back's label, its state and
  its address on the page, never its text. The text is given only on explicit
  request (`stemma readback --content [<label>…]`), when the person asks the
  agent to compare a read-back with the prose.
- A read-back is **new** (made by this run), **current** (made of the
  environment's Lean as it is now), **stale** (made of Lean that has changed
  since, or of an environment that is gone) or **archived**.
- **The page** sets each read-back beside its environment's prose and Lean.
  It has an index, groups read-backs by module in the order of the table of
  contents, filters them (unread, approved, stale, archived), and gives each
  one a stable anchor, `…/#<label>`, that other tools can link to.
- **Review marks.** On the page, the person marks each read-back unread, read
  or approved, and may write a short note on it. Marks are the person's own
  record of their review, nothing more: they live in `.stemma/reviews.json`,
  one file per person and per clone, never committed nor signed. Each mark
  records the formal fingerprint of the read-back it was made on, so that a
  mark on an earlier read-back shows as outdated. The agent reads notes only
  when the person asks (`stemma readback --notes [<label>…]`).
- **Archiving.** When `stemma sign` signs an environment whose read-back was
  made of the Lean being signed, that read-back is archived. The person can
  also archive and unarchive read-backs on the page. Archived read-backs are
  hidden unless the page is asked to show them; a read-back made of other Lean
  is not archived.
- The page writes marks through the local server that serves it, which
  listens only on 127.0.0.1, answers only requests addressed to itself, and
  accepts a write only with a random token that the page it served carries.
  It writes nowhere but `.stemma/reviews.json`.
- Read-backs are kept in `.stemma/`. They are not committed, and no signature
  requires one.

## 4. Collaboration

A library lives in a repository on a forge, with one shared branch, `main`.
Changes reach `main` only through pull requests that pass the library's
checks.

### Working

- Each person works on branches of their own: `work/<person>` by default, or
  `work/<person>-<topic>` for a separate piece of work. A working branch is
  never `work/<person>/<topic>`: git cannot hold `work/<person>` beside it.
- Starting an agent (`stemma agent <harness>`) asks which branch to
  work on: the current branch first (unless it is `main`), then the person's
  other working branches with their state (commits ahead of `main`, an open or
  merged pull request, uncommitted changes), then a new branch. It never
  offers `main` nor a share branch. A new branch starts from an up-to-date
  `main`, not from HEAD. `--branch <name>` and `--here` choose without asking;
  without a question, starting from `main` moves to `work/<person>`.
- When the current branch has uncommitted changes, the person chooses whether
  to carry them to the other branch or to stay; nothing is ever discarded.
  Without a question, it stays and says so.
- Branches whose content is all in `main` (here and where they are saved) are
  marked, and the picker offers to delete them, here and on their remote
  (`--delete-merged` without asking). Nothing else is ever deleted: not a
  branch with content missing from `main`, not one with an open pull request,
  not the current branch.
- Starting an agent does not synchronize anything: the person works, and the
  agent commits and pushes the working branch, which keeps the work safe.
- **Saving.** Working branches are pushed to the group's repository, unless
  the person sets a **personal remote** (`stemma remote <url>`, or when their
  first working branch is created). It lives in the clone's local git
  configuration, never in `stemma.toml`. With one, working branches go there,
  and only share branches go to the group's repository, so that drafts and
  failed attempts are backed up without being visible to everyone.
- A branch may mix several things: a person need not decide in advance what a
  piece of work will turn into.
- People who know git work as they like (several branches, bringing `main` in
  whenever they want). `stemma` only asks that what reaches `main` goes
  through a pull request.

### Sharing

- Each working branch `work/<x>` is shared through a **share branch**,
  `share/<x>`, in the group's repository, and the pull request comes from it.
  So the working branch stays free while the pull request is open: work goes
  on, and reaches the pull request only when it is shared again.
- **The invariant:** `share/<x>` only ever points at a commit of `work/<x>`.
  Neither `stemma` nor anyone else commits on it. Sharing moves it forward to
  HEAD, or to an earlier commit the person chooses (whose later signatures and
  approvals are then not included, and sharing says so).
- Everything sharing needs happens on the working branch. Sharing is the only
  moment that looks at `main`: it brings in what is new there, and conflicts
  are resolved on the working branch (asking the person, in mathematical
  terms, when two changes disagree about content).
- Sharing derives its state from git each time, comparing the working branch
  with `share/<x>` as it is in the group's repository (never only a local
  copy, which another machine may have left behind). There is no state file.
  - When `share/<x>` has commits the working branch lacks, made elsewhere (the
    forge's "Update branch", an accepted suggestion, another push), they are
    never overwritten silently: sharing shows them, and brings them into the
    working branch, or discards them after a confirmation that lists what is
    lost. Without a question, it stops and explains.
  - When the working branch was rewritten after sharing (a rebase, an amend)
    and every commit on `share/<x>` came from its earlier history, sharing
    asks, then replaces it with `--force-with-lease`. Otherwise those commits
    count as made elsewhere.
- Before it pushes, sharing shows the state (the branch, its environments,
  the checks, whether bringing in `main` conflicts) and works out what the
  change will lack. What the person can give (a signature or an approval of
  theirs) stops it, and offers to sign now, so that the checks do not fail
  for it. What only someone else can give is reported once the pull request
  is open. Updating an open pull request is confirmed, since the approvals of
  its current content stop counting. The title and body of a new pull request
  are the person's to edit.
- From an agent's session, sharing asks nothing: it shares HEAD, and stops
  with an explanation whenever a decision is needed. By default, an agent asks
  the person before sharing.
- Sharing from `main` moves the work to a working branch first.
- A pull request opened by an earlier version from the working branch itself
  is updated as before until it is merged; after that, sharing goes through
  the share branch.
- Sharing says how many definitions and statements the branch adds (§3, "What
  a branch brings"), and how many of them are central, so that the person sees
  what they are about to claim.
- `stemma status` says how far a branch is from `main`, as information. It
  never forces a synchronization.
- When part of a branch needs a signature and part does not, the agent offers
  to split it into two pull requests, so that the second waits without holding
  back the first. The person decides.
- Pull requests are merged with merge commits only, never squashed nor
  rebased (the forge enforces it), so that the working branch stays valid after
  its work reaches `main`, and the next pull request carries only what is new.

### Checks

| When | Who | What |
|---|---|---|
| While working | The agent, with `stemma check` | Everything, for quick feedback; it guarantees nothing |
| On every push | The forge, at once | Protected files and layout |
| On a pull request | Required checks | Everything, on `main` with the pull request applied: the build, the rules of this specification, and signatures. This is what guarantees |

`stemma check` builds the library itself, so an agent never runs Lake on its
own. Its JSON output always says how the build went: `"build": {"ok": true,
"seconds": …}` when it passes, and `"build": {"ok": false, "log": "…"}`, with
Lean's errors, when it fails.

A pull request that would leave a central environment unsigned (under the
default policy) or a signed one stale says which signatures it needs, label by
label, and is not merged until they are added to it. `stemma check` reports
them as problems; `stemma verify` says which are missing and establishes,
with git, who signed.

### Approvals

Some changes need a member's approval before they reach `main`. Each group
decides which, in `stemma.toml`:

```toml
[policy.review]
dependencies = ["maintainer"]   # lakefile.toml, lake-manifest.json, lean-toolchain
policy       = ["maintainer"]   # stemma.toml, .github/
```

- `policy.review` names kinds of change and the roles whose approval they
  need. A change's kinds come from the files it touches, exactly; there is no
  other way a change gets a kind.
- A change of the `policy` kind (the members, their keys and roles, the
  policy, the version of `stemma`, the forge's checks) always needs a
  maintainer's approval, and that cannot be turned off. Registering a key
  with `stemma key add` is one.
- A change of the `dependencies` kind needs a maintainer's approval unless the
  group says otherwise (`dependencies = []`): a new dependency is code that
  every member's machine will build and run.
- **An approval is a signed commit**, not a forge's review. `stemma sign`
  records it: an empty commit, or the commit of the signatures made at the
  same time, signed with the member's key, with a trailer naming the kinds it
  approves (`Approve: policy, dependencies`).
- **An approval names the content it approves**: a digest, made by git, of
  every file of the branch but signatures (`Approve-content:`). It counts only
  while that is the branch's content; any other change of content (a new
  commit, bringing in `main`, a rebase) leaves it behind, and it is given
  again. The checks of a pull request look at `main` with the change applied:
  an approval counts there when the branch already includes what `main` has,
  which sharing ensures.
- Any member with one of the roles may approve, including the one who made
  the change: an approval guarantees that a person looked at it.
- **The members and the policy are always read from `main`, never from the
  change**, so that a change cannot alter what judges it: a new member cannot
  approve the change that adds them.
- Signatures are not approvals: a signature is about an environment and lasts
  as long as the environment does; an approval is about a change. A change
  may need both, or neither. `stemma sign` makes both in one act.
- Merging is always a person's act: nothing is merged by itself, and once a
  change carries what it needs, anyone with permission on the forge merges it.

### Forges

The first forge Stemma supports is GitHub:

| Piece | On GitHub |
|---|---|
| `main` changes only through pull requests | A ruleset on `main`: pull requests required, no direct or forced pushes, no deletion |
| Merge commits only | The repository allows merge commits, and neither squash nor rebase merging |
| Required checks | A GitHub Actions workflow written by `stemma` (`.github/workflows/stemma.yml`) that runs `stemma check` and `stemma verify`; its check, `Stemma`, is required by the ruleset (without requiring branches to be up to date, which would make the forge commit on share branches) |
| Signatures and approvals | Signed commits, checked by `stemma verify` with git against the members' keys: GitHub's reviews play no part |
| The library's site | GitHub Pages, published from `main` by the same workflow |

- The checks run without secrets: `stemma verify` reads only the repository,
  so it gives the same answer on GitHub, on any other forge, and on a
  person's machine.
- Builds cache `.lake` between runs.
- `stemma` sets the repository up (rules, required check, merge commits only,
  workflow, Pages) when it creates the library, and `stemma upgrade` applies
  the same setup to existing libraries. When the forge's command line is
  missing or lacks permission, it says exactly what to set by hand, and the
  rest goes on.
- Everything `stemma` asks of a forge (who the person is, finding and opening
  pull requests, setting the repository up) goes through one part of it, so
  that other forges can be supported.

## 5. Agents

- `stemma agent <harness>` starts an agent harness in the library with the
  instructions, skills and permissions of the library's `stemma` version.
  It synchronizes nothing. The harnesses are Claude Code (`claude`), Codex
  (`codex`), DeepSeek Harness (`deepseek`) and OpenCode (`opencode`);
  `stemma claude` and `stemma codex` are shortcuts.
- **The equipment is neutral.** It is written once: the system instructions,
  the skills, the summary a session starts with, the files only `stemma`
  writes and the commands an agent may not run. Each harness gets a thin
  adapter that installs it the way that harness reads it, in
  `.stemma/agent/<harness>/`, written anew at each start; the neutral form is
  in `.stemma/agent/equipment/` (`instructions.md`, and `skills/<name>/SKILL.md`).
- **When a session starts**, the agent gets a short summary that costs no
  build: the branch, uncommitted changes, and work not yet in `main`. Claude
  Code and DeepSeek Harness get it from a hook passed when `stemma` starts
  them, so it is given again when a session resumes; Codex, which runs hooks
  only once a person trusts them, and OpenCode, which has no such hook, get it
  in their instructions, as it was when `stemma` started them.
- **What makes an agent started through `stemma`** is two things, which a
  person can also give a harness `stemma` does not support:
  1. its instructions contain the section titled "Working in a Stemma
     library": the text of `.stemma/agent/equipment/instructions.md`
     (`stemma agent <harness> --dry-run` writes it), with the skills of
     `.stemma/agent/equipment/skills/` available to it, or appended;
  2. every process it runs has the environment variable `STEMMA_SESSION`
     set, under which `stemma sign` and `stemma key` refuse to run and no
     command asks questions.
- **Agents started without `stemma`** notice it. `AGENTS.md`, which every
  agent reads, begins with a block `stemma` writes: an equipped agent's
  instructions contain the section that block names, and an agent whose
  instructions do not must tell the person, before doing anything else, to
  start it again with `stemma agent <harness>`. `stemma` keeps the block
  current, and `stemma check` fails when it is missing.
- **Permissions.** The agent may not write the files only `stemma` writes
  (§1), nor `signatures/`; it may not run `stemma sign` or `stemma key`, push
  to `main` or force a push. Its instructions say so to every harness, and
  `STEMMA_SESSION` keeps `stemma sign` and `stemma key` from running under
  every harness. Beyond that, each harness enforces what it can:

  | Harness | Protected files | Forbidden commands |
  |---|---|---|
  | Claude Code | Denied by its permission rules (`Edit(…)`), and by a `PreToolUse` hook | Denied by its permission rules (`Bash(…)`), and by the hook |
  | DeepSeek Harness | Denied by a `PreToolUse` hook, run through its bridge for Claude Code hooks, for its file tools (`write`, `edit`, `str_replace_editor`) | Denied by the same hook, for its `bash` tool |
  | OpenCode | Denied by its `edit` permission, which covers every tool that writes files | Denied by its `bash` permission |
  | Codex | Not enforced: its sandbox (`workspace-write`) has no rules for single files | Not enforced: `stemma` gives Codex no command rules |

  None of them stops a shell command that writes a protected file (`echo >
  stemma.toml`), and the command rules match commands by their text: they
  guard against shortcuts, not against an agent set on evading them. What
  guarantees is elsewhere: `stemma verify` accepts signatures only in commits
  signed with a signer's key (§3), and changes to the files only `stemma`
  writes need a maintainer's approval (§4).
- **Centrality.** The agent never marks an environment central without the
  person's permission. Before each sharing, and when it finishes a block of
  work, it gives the person a short list of candidates for central, with their
  role in the library, and lets them decide; when the person already marks
  central environments themselves, it proposes only strong candidates.
- **Authorship.** The person is the author of every commit. The agent adds a
  trailer naming itself:

  ```
  Agent: <model>
  ```

- **Skills** teach the agent to write documents and environments, create
  modules, share work, resolve conflicts, prepare signatures and ask for
  read-backs.

## 6. Command line

| Command | What it does |
|---|---|
| `stemma init` | Creates a library: layout, `stemma.toml`, dependencies, git and its remote; in a terminal it asks what it needs |
| `stemma new` | Creates a module (document or Lean) and adds it to the table of contents |
| `stemma check` | Runs every check of this specification |
| `stemma status` | Shows states: not formalized, pending, signatures, distance from `main` |
| `stemma preview` | Builds the site and serves it locally |
| `stemma share` | Shares the working branch through its share branch: brings `main` in, and opens or updates the pull request |
| `stemma remote` | Shows or sets the personal remote working branches are saved to |
| `stemma upgrade` | Moves the library to this version of `stemma`: migrations, the files only `stemma` writes, and the dependencies. In a terminal it lists the migrations and asks first (`--yes` does not ask); it ends saying what follows: `stemma sign`, by a maintainer, to approve the upgrade and re-sign what it left stale (`stemma status` lists them), then `stemma share` |
| `stemma verify` | Whether a change carries the signatures and approvals it needs, from git alone |
| `stemma sign` | Signs environments and approves changes, in an interactive terminal; `stemma sign <labels…>` only those |
| `stemma key add` | Registers the key a member signs with, on a branch, for a maintainer to approve |
| `stemma readback` | Makes read-backs and serves a local page for a person to read and mark them |
| `stemma agent <harness>` | Starts an equipped agent harness: `claude`, `codex`, `deepseek` or `opencode` (`stemma claude` and `stemma codex` are shortcuts) |

Every command has a `--json` output for agents.

### When a command asks

Some commands need a person's decisions (which branch to work on, what to
share). A command asks questions only when all of these hold:

- stdin and stdout are terminals;
- it is not running in an agent's session;
- there is no `--json`;
- there is no `--yes`.

Every question has an equivalent flag, so that advanced users and agents run
the same commands without prompts, and `--yes` accepts what the command would
do. Without questions, a consequential decision is never guessed: when it has
no safe default and no flag gives it, the command fails and names the flag to
pass. Decisions that discard work or force a push are a person's: they are
never taken in an agent's session, whatever the flags.

## Deferred

These are left for later versions:

- **Private work.** Beyond a personal remote (§4), there is none: working
  branches already keep work out of `main` until it is shared.
- **Readings.** Their format, how they cite the library, and where papers
  live.
- **References across libraries.** How a document cites an environment of
  another Stemma library it depends on.
