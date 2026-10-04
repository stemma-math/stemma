import Stemma.Environment.Record
import Stemma.Environment.Obligations

/-!
# Status

`#stemma_status` lists every environment visible from the current module, with
its kind, its marks and its state.
-/

namespace Stemma

open Lean Elab Command

/-- One line describing an environment and its state. -/
def describe (r : EnvRecord) : CoreM String := do
  let label := r.label.getD "-"
  let state ← match r.base with
    | .definition | .statement => do
      let s ← proofState r.decls
      pure s!"{if r.central then "central" else "not central"}, {s}"
    | .proof => pure s!"proof of {r.of.getD "?"}"
    | .remark => pure "remark"
  let decls := if r.decls.isEmpty then "" else s!" {r.decls.toList}"
  return s!"{r.display} {label}: {state}{decls}"

/-- Lists every environment visible from here, with its state. -/
elab "#stemma_status" : command => do
  let lines ← liftCoreM <| (allRecords (← getEnv)).mapM describe
  logInfo (if lines.isEmpty then "No environments." else "\n".intercalate lines.toList)

end Stemma
