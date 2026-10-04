import Lean

/-!
# Obligations and states

The obligations of an environment are the proofs its formalization needs. They
are never marked: Lean tells data and proofs apart, so they are computed.
-/

namespace Stemma

open Lean Meta

/-- The axioms of Lean's standard foundations, which every result may use. -/
def standardAxioms : List Name := [``propext, ``Classical.choice, ``Quot.sound]

/-- The state of a definition or a statement. -/
inductive ProofState where
  /-- It has no Lean. -/
  | notFormalized
  /-- Some obligation depends on `sorry`. -/
  | pending
  /-- Some obligation rests on axioms accepted from the literature. -/
  | cited
  /-- None of the above. -/
  | proved
  deriving Inhabited, BEq, Repr, ToJson, FromJson

def ProofState.toString : ProofState → String
  | .notFormalized => "not formalized"
  | .pending => "pending"
  | .cited => "cited"
  | .proved => "proved"

instance : ToString ProofState := ⟨ProofState.toString⟩

/-- Every axiom the declarations depend on, transitively. -/
def axiomsOf (decls : Array Name) : CoreM NameSet := do
  let mut out := {}
  for d in decls do
    for a in ← collectAxioms d do
      out := out.insert a
  return out

/-- The state of an environment formalized by these declarations. -/
def proofState (decls : Array Name) : CoreM ProofState := do
  if decls.isEmpty then return .notFormalized
  let axs ← axiomsOf decls
  if axs.contains ``sorryAx then return .pending
  if axs.toList.any (!standardAxioms.contains ·) then return .cited
  return .proved

/-- The constants an expression uses, skipping its proofs. -/
def dataConstants (e : Expr) : MetaM (Array Name) := do
  let found ← IO.mkRef #[]
  discard <| transform e (pre := fun e => do
    if (← try isProof e catch _ => pure false) then return .done e
    if let .const c _ := e then found.modify (·.push c)
    return .continue)
  found.get

/--
The axioms the data of a declaration depends on: its type and value, and those
of everything they use, transitively, skipping proofs.
-/
def dataAxioms (root : Name) : MetaM NameSet := do
  let env ← getEnv
  let mut todo := #[root]
  let mut visited : NameSet := {}
  let mut found : NameSet := {}
  while h : todo.size > 0 do
    let c := todo.back
    todo := todo.pop
    if visited.contains c then continue
    visited := visited.insert c
    let some ci := env.find? c | continue
    match ci with
    | .axiomInfo _ => found := found.insert c
    | .thmInfo _ => pure ()
    | _ =>
      todo := todo ++ (← dataConstants ci.type)
      if let some v := ci.value? (allowOpaque := true) then
        todo := todo ++ (← dataConstants v)
      if let .inductInfo v := ci then
        todo := todo ++ v.ctors.toArray
  return found

/--
The non-standard axioms (including `sorryAx`) the data of a declaration depends
on. Checking every axiom first keeps the common case, where there are none,
cheap.
-/
def badDataAxioms (decl : Name) : MetaM (List Name) := do
  let all ← collectAxioms decl
  if all.all (standardAxioms.contains ·) then return []
  return (← dataAxioms decl).toList.filter (!standardAxioms.contains ·)

end Stemma
