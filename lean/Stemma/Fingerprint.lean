import Lean
import Stemma.Sha256

/-!
# Fingerprints

A signature records two fingerprints of an environment: one of its prose and
one of its formal meaning.

The formal meaning of a declaration is its type and, for a definition, its
value with proofs erased: by proof irrelevance, a different proof cannot
change what is defined or established. Each constant has a *local hash* of that
content, and the formal fingerprint of an environment combines the local hashes
of everything in its *closure*: its declarations and every constant their
meaning uses, transitively, wherever it comes from.

Bound variable names, binder kinds and metadata do not count, so renaming a
bound variable changes nothing.
-/

namespace Stemma

open Lean Meta

/-- The version of the fingerprint algorithm, recorded in every signature. -/
def fingerprintVersion : Nat := 1

/-- Collapses runs of whitespace, so that spacing does not count. -/
def normalizeProse (prose : String) : String :=
  let words := (prose.split Char.isWhitespace).toList.map (·.toString)
  " ".intercalate (words.filter (!·.isEmpty))

/-- The fingerprint of an environment's prose. -/
def proseFingerprint (prose : String) : String :=
  "sha256:" ++ sha256 (normalizeProse prose)

/-- The state of a serialization: the bound variables in scope and the constants met. -/
structure SerState where
  out : String := ""
  consts : NameSet := {}

/--
Serializes an expression canonically, writing `□` for proofs and numbering bound
variables by depth.
-/
partial def serialize (e : Expr) (fvars : Array FVarId) : StateRefT SerState MetaM Unit := do
  let emit (s : String) : StateRefT SerState MetaM Unit := modify fun st => { st with out := st.out ++ s }
  if (← try isProof e catch _ => pure false) then
    emit "□"
    return
  match e with
  | .bvar i => emit s!"(B {i})"
  | .fvar id => emit s!"(V {fvars.idxOf id})"
  | .mvar _ => emit "?"
  | .sort u => emit s!"(S {u})"
  | .const n us =>
    modify fun st => { st with consts := st.consts.insert n }
    emit s!"(C {n} {us})"
  | .app f a =>
    emit "(A "; serialize f fvars; emit " "; serialize a fvars; emit ")"
  | .lam n t b bi =>
    emit "(L "; serialize t fvars; emit " "
    withLocalDecl n bi t fun x => serialize (b.instantiate1 x) (fvars.push x.fvarId!)
    emit ")"
  | .forallE n t b bi =>
    emit "(P "; serialize t fvars; emit " "
    withLocalDecl n bi t fun x => serialize (b.instantiate1 x) (fvars.push x.fvarId!)
    emit ")"
  | .letE n t v b _ =>
    emit "(E "; serialize t fvars; emit " "; serialize v fvars; emit " "
    withLetDecl n t v fun x => serialize (b.instantiate1 x) (fvars.push x.fvarId!)
    emit ")"
  | .lit (.natVal n) => emit s!"(N {n})"
  | .lit (.strVal s) => emit s!"(T {repr s})"
  | .mdata _ e => serialize e fvars
  | .proj s i e =>
    modify fun st => { st with consts := st.consts.insert s }
    emit s!"(J {s} {i} "; serialize e fvars; emit ")"

/-- The local hash of a constant, and the constants its meaning uses directly. -/
structure ConstHash where
  hash : String
  uses : Array Name
  deriving Inhabited

/-- Serializes the meaning of a constant: its type, and the value of a definition. -/
def serializeConst (ci : ConstantInfo) : StateRefT SerState MetaM Unit := do
  let kind := match ci with
    | .axiomInfo _ => "axiom" | .defnInfo _ => "def" | .thmInfo _ => "theorem"
    | .opaqueInfo _ => "opaque" | .quotInfo _ => "quot" | .inductInfo _ => "inductive"
    | .ctorInfo _ => "constructor" | .recInfo _ => "recursor"
  modify fun st => { st with out := s!"{kind} {ci.levelParams.length} " }
  serialize ci.type #[]
  match ci with
  | .defnInfo v =>
    modify fun st => { st with out := st.out ++ " := " }
    serialize v.value #[]
  | .opaqueInfo v =>
    modify fun st => { st with out := st.out ++ " := " }
    serialize v.value #[]
  | .inductInfo v =>
    for c in v.ctors do
      modify fun st => { st with out := st.out ++ s!" | {c}", consts := st.consts.insert c }
  | _ => pure ()

/-- Computes the local hash of a constant. -/
def constHash (ci : ConstantInfo) : MetaM ConstHash := do
  let (_, st) ← (serializeConst ci).run {}
  return { hash := "sha256:" ++ sha256 st.out, uses := st.consts.toArray }

/-- Local hashes, computed once per constant. -/
abbrev HashCache := Std.HashMap Name ConstHash

/-- The local hash of a constant, from the cache when it is there. -/
def cachedHash (cache : IO.Ref HashCache) (n : Name) : MetaM (Option ConstHash) := do
  if let some h := (← cache.get)[n]? then return some h
  let some ci := (← getEnv).find? n | return none
  let h ← constHash ci
  cache.modify (·.insert n h)
  return some h

/-- The closure of some declarations, with the local hash of each constant in it. -/
def closure (cache : IO.Ref HashCache) (decls : Array Name) : MetaM (Array (Name × String)) := do
  let mut todo := decls
  let mut seen : NameSet := {}
  let mut out := #[]
  while h : todo.size > 0 do
    let n := todo.back
    todo := todo.pop
    if seen.contains n then continue
    seen := seen.insert n
    let some ch ← cachedHash cache n | continue
    out := out.push (n, ch.hash)
    todo := todo ++ ch.uses
  return out.qsort (fun a b => a.1.toString < b.1.toString)

/-- The formal fingerprint of a closure. -/
def formalFingerprint (closure : Array (Name × String)) : String :=
  "sha256:" ++ sha256 ("\n".intercalate (closure.toList.map fun (n, h) => s!"{n} {h}"))

end Stemma
