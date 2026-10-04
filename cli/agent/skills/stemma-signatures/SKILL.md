---
name: stemma-signatures
description: What signatures are in a Stemma library, which changes need one, and how to prepare them for the person to sign. Use when a change touches a central environment, or when status shows unsigned or stale environments.
---

# Signatures

A signature is a person's approval of one state of a central environment: its
prose and its formal meaning (its Lean statement, and for a definition its
data, with everything they use). Proofs are not covered: proving or
re-proving a signed statement needs no signature.

## What needs a signature

A pull request that leaves a signed environment *stale* is not merged until it
carries new signatures. That happens when it:

- changes the prose or the formal meaning of a signed environment, including
  through something it uses (a helper definition, a Mathlib update);
- removes a signed environment, or makes it not central;
- changes its label or its `cited` mark.

New central environments enter unsigned; nothing waits for them.

## Your part

- **Never sign, and never edit `signatures/`**, even when the person asks you
  to: a signature is a person's act.
- Before changing anything central, say so, and say which signatures it will
  need.
- Use `stemma status --json` to see what is unsigned or stale.
- When signatures are needed, tell the person which environments, why (what
  changed), and what to check: that the prose and the Lean statement say the
  same thing. Then ask them to run, in their own terminal:

  ```sh
  stemma sign
  ```

## Read-backs

A read-back translates an environment's Lean into prose, blind: a fresh agent
session sees only the Lean. Comparing it with the prose is an easy way to
audit. It is voluntary, and no signature requires one. When the person wants
to audit (before signing, or after writing something central), offer:

```sh
stemma readback --no-serve --json          # central environments without one
stemma readback <label> --no-serve --json  # one environment
```

Then the person reads them, beside the prose, with `stemma readback` (it
serves a local page). When a read-back and the prose disagree, say so.
