import Lean

/-!
# Environment records

Every mathematical environment in a document leaves a record: its kind, its
marks, its prose and the declarations that formalize it. Records are exported,
so a module sees the records of every module it imports.
-/

namespace Stemma

open Lean

/-- The base kind of an environment, which decides how Stemma treats it. -/
inductive BaseKind where
  /-- Introduces a concept or a construction. -/
  | definition
  /-- Claims something. -/
  | statement
  /-- The informal proof of a definition or a statement. -/
  | proof
  /-- A remark, an example, a note. -/
  | remark
  deriving Inhabited, BEq, Repr, ToJson, FromJson

def BaseKind.toString : BaseKind → String
  | .definition => "definition"
  | .statement => "statement"
  | .proof => "proof"
  | .remark => "remark"

instance : ToString BaseKind := ⟨BaseKind.toString⟩

/-- What a document records about one of its environments. -/
structure EnvRecord where
  /-- The directive's name, such as `lemma`. -/
  name : Name
  /-- The name shown to readers, such as "Lemma". -/
  display : String
  base : BaseKind
  label : Option String
  central : Bool
  cited : Option String
  title : Option String
  /-- For a proof, the label of what it proves. -/
  of : Option String
  /-- The declarations its Lean adds, in order. -/
  decls : Array Name
  /-- Its prose: the source of every block but Lean code. -/
  prose : String
  /-- The source of its Lean blocks. -/
  lean : String
  module : Name
  line : Nat
  deriving Inhabited, Repr, ToJson, FromJson

initialize recordExt : SimplePersistentEnvExtension EnvRecord (Array EnvRecord) ←
  registerSimplePersistentEnvExtension {
    addEntryFn := Array.push
    addImportedFn := fun _ => #[]
  }

/-- The records of every imported module, then those of the current one. -/
def allRecords (env : Environment) : Array EnvRecord := Id.run do
  let mut out := #[]
  for i in [:env.allImportedModuleNames.size] do
    out := out ++ recordExt.getModuleEntries env i
  return out ++ recordExt.getState env

/-- The record with a given label, if any. -/
def findRecord? (env : Environment) (label : String) : Option EnvRecord :=
  (allRecords env).find? (·.label == some label)

end Stemma
