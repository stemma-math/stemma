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
work to reach the library, commit everything and run:

```sh
stemma share --json
```

It checks the library, brings `main` in, checks again, pushes the working
branch and opens or updates the pull request. Its answer says what happened:

- `conflicts`: bringing in `main` left conflicts in these files. Resolve them.
  A conflict about content (two people changed the same statement or prose
  differently) is the person's decision: explain it in mathematical terms,
  show both versions, recommend, and ask. Imports in the table of contents
  that conflict usually just both stay. Then commit and share again.
- `problems` or `build`: a check fails after bringing in `main`. Fix it,
  commit, and share again.
- `pull_request`: the pull request's address. When it is empty, `gh` could
  not open it: tell the person.
- `needs_signatures`: signed environments the pull request leaves stale. It
  will not be merged until they are signed (see `stemma-signatures`).

If part of the work needs signatures and part does not, offer to split it into
two pull requests, so that the second waits without holding back the first.
The person decides.

Never squash, never force-push, never push to `main`.
