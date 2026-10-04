---
name: stemma-sharing
description: How work is saved and shared in a Stemma library — the person's working branch, commits, bringing in main, resolving conflicts, and pull requests. Use when committing, when the person wants to share work, or when something conflicts.
---

# Saving and sharing

## Working

- Work happens on the person's branch (`work/<person>`), which `stemma`
  switches to when it starts you. Do not synchronize with `main` while
  working.
- Commit often, and push the working branch: it keeps the work safe and
  visible to the group. Messages say the mathematical step, and end with the
  `Agent: <model>` trailer.

## Sharing

Sharing is the only moment that looks at `main`. When the person wants their
work to reach the library:

1. `stemma check --json` must pass.
2. Bring `main` in: `git fetch origin`, then `git merge origin/main`.
3. Resolve conflicts. A conflict about content (two people changed the same
   statement differently) is the person's decision: explain it in
   mathematical terms, show both versions, recommend, and ask. Imports in the
   table of contents that conflict usually just both stay.
4. `stemma check --json` again, commit, push.
5. Open the pull request from the working branch to `main` (`gh pr create`),
   or update the open one. Its description lists the mathematical changes,
   the decisions taken, and the signatures it needs (see `stemma-signatures`).
6. If part of the work needs signatures and part does not, offer to split it
   into two pull requests, so that the second waits without holding back the
   first. The person decides.

Never squash, never force-push, never push to `main`.
