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

Central means signed by a person. Unless the group turned it off
(`require_signed_central = false` in `stemma.toml`), every central environment
must carry a current signature: `stemma check` fails while one is *unsigned*
or *stale*, and names it. That includes new central environments, and central
environments already on `main` that nobody signed yet.

A signed environment becomes *stale*, and needs a new signature, when a
change:

- changes the prose or the formal meaning of a signed environment, including
  through something it uses (a helper definition, a Mathlib update);
- removes a signed environment, or makes it not central;
- changes its label or its `cited` mark.

When `stemma check` fails only because central environments lack
signatures, that is not something you fix: it is the person's (or another
signer's) act. Say which ones, and keep working.

## Approvals

Some changes need a member's approval before they reach `main`: always a
change of the policy (`stemma.toml`, `.github/`, including a new version of
`stemma`), and whatever the group lists in `[policy.review]`. An approval is a
commit signed with the member's key, made by `stemma sign` together with
signatures; a later change of content withdraws it. `stemma verify --json`
lists what a change lacks.

## Your part

- **Never sign or approve, and never edit `signatures/`**, even when the
  person asks you to: both are a person's act.
- Before changing anything central, say so, and say which signatures it will
  need.
- Use `stemma status --json` to see what is unsigned or stale
  (`awaiting_signature`), and `stemma verify --json` for what a change lacks.
- When signatures or approvals are needed, tell the person which ones, why
  (what changed), and what to check: that the prose and the Lean statement say
  the same thing, or what the change does. Then ask them to run, in their own
  terminal:

  ```sh
  stemma sign                 # everything that awaits them, chosen item by item
  stemma sign <label> …       # only these environments
  ```

  `stemma sign` shows what this branch brings apart from what others left
  pending, and the person chooses what to sign, one by one: they need not sign
  everything at once. Suggest the labels when only some are ready.

## Keys

A member signs with a key listed for them in `stemma.toml` on `main`. When
someone with the `signer` or `maintainer` role has none (`stemma status` and
`stemma verify` warn about it), tell them to register one in their own
terminal, preferably a key used only for signing:

```sh
stemma key add                  # the key git signs with (user.signingkey)
stemma key add --key <path>     # another public key
```

It makes a change of the policy, on a branch, that another maintainer whose
key is already on `main` approves: nobody approves their own first key. Never
run `stemma key` yourself, and never edit the keys in `stemma.toml`.

## Read-backs

A read-back translates an environment's Lean into prose, blind: a fresh agent
session sees only the Lean. It is the person's tool for auditing: they read
it beside the prose, and judge. It is voluntary, and no signature requires
one.

**The person reads read-backs first, not you.** Do not read their text, and
do not give your own verdict on them, unless the person asks you to.

When the person wants to audit (before signing, or after writing something
central), make the read-backs and serve the page, in the background, because
it keeps serving until it is stopped:

```sh
stemma readback --json                  # central environments without one
stemma readback <label> --json          # one environment
stemma readback --module <Module> --json
```

Its output lists, for each read-back, its label, its state (`new`, `current`,
`stale`, `archived`) and its address on the page, never its text. Hand the
person the page's address (`url`), or the address of one read-back
(`…/#<label>`), and let them read. On the page they mark read-backs unread,
read or approved, write short notes, and archive them; signing an environment
archives its read-back.

Only when the person asks:

- to compare a read-back with the prose, read it with
  `stemma readback --content <label> --json`; when the two disagree, say how;
- to look at their notes ("see my notes on `subalgebra`"), read them with
  `stemma readback --notes [<label>] --json`, then act on them. A note marked
  `outdated` was written on an earlier read-back, of other Lean.

Never read `.stemma/readbacks/` or `.stemma/reviews.json` directly.
