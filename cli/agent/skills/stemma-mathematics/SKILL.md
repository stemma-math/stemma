---
name: stemma-mathematics
description: How to do mathematics in a Stemma library — finding what exists, stating precisely, statements before proofs, and keeping prose and Lean saying the same thing. Use whenever the person wants to define, state, prove or explore something.
---

# Doing mathematics

## The cycle for each request

0. **Search before creating.** Look in Mathlib (and in the library) for the
   concept or result. If it exists, is a renaming or a special case: stop, name
   the declarations you found and what surrounds them, and let the person
   decide (build on it, or define the group's own).
1. **Answer first**, before touching files:
   - the precise statement;
   - a proof sketch;
   - where it fits in the library;
   - design decisions, with alternatives and a recommendation;
   - Mathlib conventions that change the meaning.
2. **Write it in a document**: the environment, its prose, and its Lean with
   the statement and `sorry` as proof. Definitions must be complete: no
   `sorry` in their data. Run `stemma check --json`.
3. **Wait for the person to agree with central statements** before spending
   effort on their proofs, unless the proof is genuinely trivial.
4. **Prove.** Put helpers the reader does not need outside environments, or
   in a Lean module (`stemma new … --lean`). Prove one step at a time; read
   errors; work on the hardest case first.
5. **Align the prose.** If the proof took another route than the one the
   prose describes, update the prose (and the informal `proof` environment).
   Never change a central statement to make a proof go through: tell the
   person, and propose the change.
6. **Check and commit**: `stemma check --json`, then commit with a
   mathematical message and the `Agent:` trailer.

## Keep in mind

- The library is the group's own: results stay in it. Do not propose moving
  them to Mathlib.
- A literature result that will not be formalized is a `cited` environment
  whose obligations are named `axiom`s, never a silent `sorry`.
- When something is central and changes, its signature becomes stale: tell
  the person (see `stemma-signatures`).
