import Lean
import Stemma.Environment.Record
import Stemma.Environment.Obligations
import Stemma.Fingerprint

/-!
# Extraction

The report Stemma extracts from a compiled library: its modules, every
environment with its state and fingerprints, and the problems only a view of
the whole library can find. The command line reads it as JSON.
-/

namespace Stemma

open Lean Meta

/-- The name of the constant every Verso document defines. -/
def documentConstant (module : Name) : Name :=
  module ++ Name.mkSimple "the canonical document object name"

structure ModuleReport where
  name : Name
  /-- `document` or `lean`. -/
  kind : String
  deriving ToJson, FromJson

structure Fingerprints where
  version : Nat
  prose : String
  formal : String
  deriving ToJson, FromJson

structure EnvReport where
  record : EnvRecord
  /-- The state of a definition or a statement. -/
  state : Option ProofState
  /-- The fingerprints of a formalized definition or statement. -/
  fingerprints : Option Fingerprints
  /--
  The local hash of each constant in the closure that belongs to the library,
  or that something in the library uses directly.
  -/
  closure : Array (Name × String)
  deriving ToJson

structure Diagnostic where
  message : String
  module : Option Name := none
  line : Option Nat := none
  deriving ToJson, FromJson

structure Report where
  version : Nat := 1
  library : Name
  modules : Array ModuleReport
  environments : Array EnvReport
  diagnostics : Array Diagnostic
  deriving ToJson

/-- The module that declares a constant, when it is imported. -/
def moduleOf? (env : Environment) (n : Name) : Option Name :=
  env.getModuleIdxFor? n |>.map fun i => env.header.moduleNames[i.toNat]!

/-- Whether a module belongs to the library. -/
def inLibrary (root module : Name) : Bool := root.isPrefixOf module

/-- The line of a declaration, when Lean knows it. -/
def declLine (n : Name) : MetaM (Option Nat) := do
  return (← findDeclarationRanges? n).map (·.range.pos.line)

/-- The modules of the library, in the order of the table of contents when the root is loaded. -/
def libraryModules (env : Environment) (root : Name) : Array Name := Id.run do
  let all := env.allImportedModuleNames.filter fun m => inLibrary root m && m != root
  let some idx := env.getModuleIdx? root | return all
  let listed := env.header.moduleData[idx.toNat]!.imports.map (·.module) |>.filter (inLibrary root)
  return listed ++ all.filter (!listed.contains ·)

/-- Problems in the table of contents: modules the root does not import. -/
def tocDiagnostics (env : Environment) (root : Name) : Array Diagnostic := Id.run do
  let some idx := env.getModuleIdx? root | return #[]
  let listed := env.header.moduleData[idx.toNat]!.imports.map (·.module)
  let mut out := #[]
  for m in env.allImportedModuleNames do
    if inLibrary root m && m != root && !listed.contains m then
      out := out.push { message := s!"The module {m} is missing from the table of contents \
        ({root}.lean)." }
  return out

/-- Labels used by more than one environment. -/
def labelDiagnostics (records : Array EnvRecord) : Array Diagnostic := Id.run do
  let mut seen : Std.HashMap String EnvRecord := {}
  let mut out := #[]
  for r in records do
    let some label := r.label | continue
    if let some first := seen[label]? then
      out := out.push {
        message := s!"The label '{label}' is used twice: here and in {first.module} \
          (line {first.line})."
        module := r.module, line := r.line }
    else
      seen := seen.insert label r
  return out

/-- Axioms outside every environment: they are allowed only in cited ones. -/
def axiomDiagnostics (env : Environment) (root : Name) (records : Array EnvRecord) :
    MetaM (Array Diagnostic) := do
  let inEnvironments : NameSet := records.foldl (fun s r => r.decls.foldl NameSet.insert s) {}
  let mut out := #[]
  for (n, ci) in env.constants.map₁.toList do
    let .axiomInfo _ := ci | continue
    if n.isInternal || inEnvironments.contains n then continue
    let some m := moduleOf? env n | continue
    if !inLibrary root m then continue
    out := out.push {
      message := s!"The axiom '{n}' is outside every environment: axioms are allowed only in \
        an environment marked 'cited'."
      module := m, line := ← declLine n }
  return out

/-- The report of one environment. -/
def envReport (env : Environment) (root : Name) (cache : IO.Ref HashCache) (r : EnvRecord) :
    MetaM EnvReport := do
  let isClaim := r.base == .definition || r.base == .statement
  if !isClaim then return { record := r, state := none, fingerprints := none, closure := #[] }
  let state ← proofState r.decls
  if r.decls.isEmpty then return { record := r, state, fingerprints := none, closure := #[] }
  let full ← closure cache r.decls
  -- Keep the library's constants, and those the library's constants use directly.
  let mut shown : NameSet := {}
  for (n, _) in full do
    if (moduleOf? env n).all (inLibrary root) || r.decls.contains n then
      shown := shown.insert n
      if let some h := (← cache.get)[n]? then
        for u in h.uses do shown := shown.insert u
  let fingerprints := some {
    version := fingerprintVersion
    prose := proseFingerprint r.prose
    formal := formalFingerprint full }
  return { record := r, state, fingerprints, closure := full.filter (shown.contains ·.1) }

/-- Extracts the report of the library whose root module is `root`. -/
def extract (root : Name) : MetaM Report := do
  let env ← getEnv
  let modules := libraryModules env root |>.map fun m =>
    { name := m, kind := if env.contains (documentConstant m) then "document" else "lean" }
  let records := allRecords env |>.filter (inLibrary root ·.module)
  let cache ← IO.mkRef ({} : HashCache)
  let environments ← records.mapM (envReport env root cache)
  let diagnostics :=
    tocDiagnostics env root ++ labelDiagnostics records ++ (← axiomDiagnostics env root records)
  return { library := root, modules, environments, diagnostics }

end Stemma
