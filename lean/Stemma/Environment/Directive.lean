import VersoManual
import Stemma.Environment.Record
import Stemma.Environment.Obligations

/-!
# Environment directives

A mathematical environment is a Verso directive (`:::theorem … :::`). Its
directive parses the environment's marks, elaborates its contents, records the
declarations its Lean adds, and enforces the rules of the specification.

Environments are registered with `register_environment`, which defines the
directive in the `Stemma` namespace, so that the `open Stemma` of a document's
header makes it available.
-/

namespace Stemma

open Lean Elab Meta
open Verso Doc Elab ArgParse
open Verso.Genre Manual

/-- The marks of a definition or a statement. -/
structure ClaimMarks where
  label : String
  central : Bool
  title : Option String
  cited : Option String

instance : FromArgs ClaimMarks DocElabM where
  fromArgs :=
    ClaimMarks.mk <$> .named `label .string false <*> .named `central .bool false <*>
      .named `title .string true <*> .named `cited .string true <* .done

/-- The marks of a proof. -/
structure ProofMarks where
  of : String
  label : Option String
  title : Option String

instance : FromArgs ProofMarks DocElabM where
  fromArgs :=
    ProofMarks.mk <$> .named `of .string false <*> .named `label .string true <*>
      .named `title .string true <* .done

/-- The marks of a remark. -/
structure RemarkMarks where
  label : Option String
  title : Option String

instance : FromArgs RemarkMarks DocElabM where
  fromArgs := RemarkMarks.mk <$> .named `label .string true <*> .named `title .string true <* .done

/-- The marks every environment may have, whatever its base kind. -/
structure Marks where
  label : Option String := none
  central : Bool := false
  title : Option String := none
  cited : Option String := none
  of : Option String := none

block_extension Block.environment (display base : String) (label title : Option String) where
  data := toJson (display, base, label, title)
  traverse _ _ _ := pure none
  toTeX := some fun _ goB _ _ content => content.mapM goB
  toHtml :=
    open Verso.Output.Html in
    some fun _ goB _ data content => do
      let .ok ((display, base, label, title) : String × String × Option String × Option String) :=
          fromJson? data
        | pure .empty
      let heading : Verso.Output.Html := match title with
        | some t => {{<strong>{{display}}</strong>" (" {{t}} ")."}}
        | none => {{<strong>{{display}}</strong>"."}}
      pure {{
        <div class={{s!"stemma-env stemma-{base}"}} id={{label.getD ""}}>
          <p class="stemma-env-heading">{{heading}}</p>
          {{← content.mapM goB}}
        </div>
      }}

/-- Labels are lowercase letters, digits, `-` and `.`, starting with a letter or digit. -/
def validLabel (label : String) : Bool :=
  match label.toList with
  | [] => false
  | c :: cs =>
    (c.isLower || c.isDigit) && cs.all fun c => c.isLower || c.isDigit || c == '-' || c == '.'

/-- The last components of the declarations Lean generates for an inductive type. -/
def generatedSuffixes : List String :=
  ["noConfusionType", "ctorIdx", "sizeOf_spec", "inj", "injEq", "eq_def"]

/-- Whether a declaration is one a person wrote, rather than one Lean generated. -/
def isUserDecl (env : Environment) (n : Name) : Bool :=
  let generatedName := match n with
    | .str _ s => generatedSuffixes.contains s || s.startsWith "eq_"
    | _ => false
  let generatedKind := match env.find? n with
    | some (.ctorInfo _) | some (.recInfo _) => true
    | _ => false
  !n.isInternal && !generatedName && !generatedKind && !isAuxRecursor env n &&
    !isNoConfusion env n && !isMatcherCore env n && !(env.isProjectionFn n)

/-- The source of every block but Lean code: an environment's prose. -/
def proseOf (contents : TSyntaxArray `block) : DocElabM String := do
  let text ← getFileMap
  let mut parts := #[]
  for b in contents do
    if b.raw.getKind == ``Lean.Doc.Syntax.codeblock then continue
    if let (some s, some e) := (b.raw.getPos?, b.raw.getTailPos?) then
      parts := parts.push (String.Pos.Raw.extract text.source s e).trimAscii.toString
  return "\n\n".intercalate parts.toList

/-- Checks the marks of an environment before its contents are elaborated. -/
def checkMarks (base : BaseKind) (marks : Marks) : DocElabM Unit := do
  let env ← getEnv
  if let some label := marks.label then
    unless validLabel label do
      throwError m!"Invalid label '{label}': labels use lowercase letters, digits, '-' and '.'."
    if let some other := findRecord? env label then
      throwError m!"The label '{label}' is already used, in {other.module} (line {other.line})."
  if let some target := marks.of then
    let some r := findRecord? env target
      | throwError m!"A proof must name, with 'of', an existing definition or statement; \
          there is no environment labelled '{target}'."
    unless r.base == .definition || r.base == .statement do
      throwError m!"A proof must name a definition or a statement, but '{target}' is a {r.base}."
  if marks.cited.isSome && !(base == .definition || base == .statement) then
    throwError "Only definitions and statements can be cited."

/-- Checks the declarations an environment added. -/
def checkDecls (marks : Marks) (decls : Array Name) : DocElabM Unit := do
  let env ← getEnv
  for d in decls do
    let some ci := env.find? d | continue
    match ci with
    | .axiomInfo _ =>
      if marks.cited.isNone then
        logError m!"The axiom '{d}' is not allowed: axioms are allowed only in an \
          environment marked 'cited'."
      else unless ← isProp ci.type do
        logError m!"The axiom '{d}' is not a proposition: a cited environment may accept \
          obligations from the literature, never objects."
    | .thmInfo _ => pure ()
    | _ =>
      let bad ← badDataAxioms d
      if bad.contains ``sorryAx then
        logError m!"The data of '{d}' depends on 'sorry'. 'sorry' is allowed only in proofs: \
          in data it would change what is defined."
      for a in bad.filter (· != ``sorryAx) do
        logError m!"The data of '{d}' depends on the axiom '{a}'. Data never depends on axioms."

/--
Expands an environment: checks its marks, elaborates its contents, records the
declarations they add and checks them.
-/
def expandEnvironment (name : Name) (display : String) (base : BaseKind) (marks : Marks)
    (contents : TSyntaxArray `block) : DocElabM Term := do
  let ref ← getRef
  checkMarks base marks
  let before := (← getEnv).constants.map₂
  let blocks ← contents.mapM elabBlock
  let env ← getEnv
  let mut added := #[]
  for (n, _) in env.constants.map₂.toList do
    if !before.contains n && isUserDecl env n then
      let pos := (← findDeclarationRanges? n).map (·.range.pos) |>.getD ⟨0, 0⟩
      added := added.push (pos, n)
  let decls := added.qsort (fun a b => a.1.line < b.1.line ||
    (a.1.line == b.1.line && a.1.column < b.1.column)) |>.map (·.2)
  checkDecls marks decls
  let line := (← getFileMap).toPosition (ref.getPos?.getD 0) |>.line
  modifyEnv (recordExt.addEntry · {
    name, display, base, decls, line
    label := marks.label, central := marks.central, cited := marks.cited
    title := marks.title, of := marks.of
    prose := ← proseOf contents
    module := env.mainModule
  })
  ``(Verso.Doc.Block.other
      (Block.environment $(quote display) $(quote base.toString) $(quote marks.label)
        $(quote marks.title))
      #[$blocks,*])

/-- Expands a definition or a statement. -/
def expandClaim (name : Name) (display : String) (base : BaseKind) :
    DirectiveExpanderOf ClaimMarks
  | m, contents => expandEnvironment name display base
      { label := m.label, central := m.central, title := m.title, cited := m.cited } contents

/-- Expands a proof. -/
def expandProof (name : Name) (display : String) : DirectiveExpanderOf ProofMarks
  | m, contents => expandEnvironment name display .proof
      { label := m.label, title := m.title, of := m.of } contents

/-- Expands a remark. -/
def expandRemark (name : Name) (display : String) : DirectiveExpanderOf RemarkMarks
  | m, contents => expandEnvironment name display .remark
      { label := m.label, title := m.title } contents

/--
`register_environment name : base "Display"` defines the environment
`:::name`, of the given base kind (`definition`, `statement`, `proof` or
`remark`), shown to readers as "Display".
-/
syntax (name := registerEnvironment) "register_environment " ident " : " ident str : command

macro_rules
  | `(register_environment $name : $base $display) => do
    let declName := mkIdentFrom name (`Stemma ++ name.getId)
    let nameLit := quote name.getId
    match base.getId with
    | `definition =>
      `(@[directive] def $declName := expandClaim $nameLit $display .definition)
    | `statement =>
      `(@[directive] def $declName := expandClaim $nameLit $display .statement)
    | `proof => `(@[directive] def $declName := expandProof $nameLit $display)
    | `remark => `(@[directive] def $declName := expandRemark $nameLit $display)
    | _ =>
      Macro.throwErrorAt base
        ("The base kind must be one of 'definition', 'statement', 'proof' or 'remark'.")

end Stemma
