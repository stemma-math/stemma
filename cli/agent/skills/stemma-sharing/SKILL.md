---
name: stemma-sharing
description: How work is saved and shared in a Stemma library — the person's working branch, commits, its share branch, bringing in main, resolving conflicts, and pull requests. Use when committing, when the person wants to share work, or when something conflicts.
---

# Saving and sharing

## Working

- Work happens on one of the person's working branches (`work/<person>`, or
  `work/<person>-<topic>`), which the person chose when `stemma` started you.
  Never work on `main`, nor on a share branch (`share/…`). Do not synchronize
  with `main` while working.
- Commit often. Messages say the mathematical step, and end with the
  `Agent: <model>` trailer.
- Save the work with `git push` (no remote named). It goes where the person
  saves working branches: their personal remote when they set one (then work
  in progress is not visible to the group), or the group's repository.
- Never push a share branch yourself, never commit on one, and never push to
  `main` or force a push: `stemma share` does what reaches the group.

## When to share

By default, **ask the person before every `stemma share`**: sharing makes the
work visible to the group and updates its pull request. If the person has
given standing permission ("share whenever a result is done"), or said when to
share, follow that instead. This is an instruction, not something `stemma`
enforces.

## Sharing

Each working branch `work/<x>` is shared through `share/<x>`: the pull request
comes from it, and the working branch stays free while it is open. Sharing
moves `share/<x>` forward to a commit of `work/<x>`; everything it needs
(bringing `main` in, signatures, approvals) happens on `work/<x>`.

When the person wants their work to reach the library, first put the
centrality decision in front of them (see `stemma-mathematics`, "Proposing
what is central"): a short list of candidates among what the branch adds,
unless they already decided. Then commit everything and run:

```sh
stemma share --json
```

It brings `main` into the working branch, checks the library, pushes
`share/<x>` and opens or updates the pull request. You share HEAD; it asks
nothing. Its answer says what happened:

- `conflicts`: bringing in `main` (or commits from the share branch) left
  conflicts in these files. Resolve them. A conflict about content (two people
  changed the same statement or prose differently) is the person's decision:
  explain it in mathematical terms, show both versions, recommend, and ask.
  Imports in the table of contents that conflict usually just both stay. Then
  commit and share again.
- `problems`, or a `build` whose `ok` is false (its `log` has Lean's
  errors): a check fails after bringing in `main`. Fix it, commit, and share
  again.
- `stopped`: sharing needs a decision and pushed nothing. Tell the person
  what it says. In particular:
  - the share branch has commits made elsewhere (`foreign`), such as the
    forge's "Update branch" or a suggestion accepted on the pull request.
    Bringing them in loses nothing: with the person's agreement, run
    `stemma share --json --foreign merge`. Discarding them is the person's
    decision, in their own terminal.
  - the working branch was rewritten after it was shared: replacing the shared
    history forces a push, which only the person does, with `stemma share` in
    their own terminal.
- `pull_request`: the pull request's address. When it is empty,
  `forge_error` says why the forge could not open it: tell the person.
- `needs_you`: what the change lacks that the person can give: their
  signature of an environment, or their approval of the change (a change of
  the policy, of versions…). Nothing was pushed. Tell them what it is and why,
  and ask them to run `stemma sign` in their own terminal; then share again.
- `needs_others`: what only someone else can give. The pull request is open;
  tell the person whose signature or approval it waits for.
- `new_environments`: how many definitions and statements the branch adds
  (`count`, `labels`), and how many of them are central (`central`,
  `central_labels`). Say it to the person in one line.

If part of the work needs signatures and part does not, offer to split it into
two pull requests, so that the second waits without holding back the first.
The person decides; in their terminal, `stemma share` can also share up to an
earlier commit.

Never squash, never force-push, never push to `main`.
