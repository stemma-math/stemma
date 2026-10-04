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

/-- The style of environments on the site. -/
def environmentCss : String := r#"
.stemma-env { margin: 1.2em 0; }
.stemma-env-heading { margin-bottom: 0.3em; }
.stemma-statement > p, .stemma-statement > ul, .stemma-statement > ol { font-style: italic; }
.stemma-statement > .stemma-env-heading { font-style: normal; }
.stemma-proof > p:last-of-type::after { content: "∎"; float: right; }
.stemma-proof-of { font-size: 0.85em; color: var(--verso-text-color, #555); opacity: 0.7; }
.stemma-formalization { margin-top: 0.5em; border-left: 3px solid #d0d7de; padding-left: 0.8em; }
.stemma-formalization > summary { cursor: pointer; font-size: 0.85em; opacity: 0.7; }
"#

block_extension Block.environment (display base : String) (number : Option Nat)
    (label title of : Option String) where
  data := toJson (display, base, number, label, title, of)
  extraCss := [environmentCss]
  traverse _ _ _ := pure none
  toTeX := some fun _ goB _ _ content => content.mapM goB
  toHtml :=
    open Verso.Output.Html in
    some fun _ goB _ data content => do
      let .ok ((display, base, number, label, title, of) :
          String × String × Option Nat × Option String × Option String × Option String) :=
          fromJson? data
        | pure .empty
      let name := match number with
        | some n => s!"{display} {n}"
        | none => display
      let name : Verso.Output.Html :=
        if base == "proof" then {{<em>{{name}}</em>}} else {{<strong>{{name}}</strong>}}
      let heading : Verso.Output.Html := match title with
        | some t => {{{{name}}" (" {{t}} ")."}}
        | none => {{{{name}}"."}}
      let ofLink : Verso.Output.Html := match of with
        | some target => {{" "<a class="stemma-proof-of" href={{s!"#{target}"}}>{{s!"of {target}"}}</a>}}
        | none => .empty
      pure {{
        <div class={{s!"stemma-env stemma-{base}"}} id={{label.getD ""}}>
          <p class="stemma-env-heading">{{heading}}{{ofLink}}</p>
          {{← content.mapM goB}}
        </div>
      }}

block_extension Block.formalization where
  traverse _ _ _ := pure none
  toTeX := some fun _ goB _ _ content => content.mapM goB
  toHtml :=
    open Verso.Output.Html in
    some fun _ goB _ _ content => do
      pure {{
        <details class="stemma-formalization">
          <summary>"Lean"</summary>
          {{← content.mapM goB}}
        </details>
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

/--
The source of an environment's blocks: those of Lean code when `lean` is true,
and the others, its prose, when it is false.
-/
def sourceOf (contents : TSyntaxArray `block) (lean : Bool) : DocElabM String := do
  let text ← getFileMap
  let mut parts := #[]
  for b in contents do
    if (b.raw.getKind == ``Lean.Doc.Syntax.codeblock) != lean then continue
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
  let numbered := (recordExt.getState (← getEnv)).filter (·.base != .proof) |>.size
  let number := if base == .proof then none else some (numbered + 1)
  let mut prose := #[]
  let mut lean := #[]
  for b in contents do
    let term ← elabBlock b
    if b.raw.getKind == ``Lean.Doc.Syntax.codeblock then lean := lean.push term
    else prose := prose.push term
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
    prose := ← sourceOf contents (lean := false)
    lean := ← sourceOf contents (lean := true)
    module := env.mainModule
  })
  let blocks ← if lean.isEmpty then pure prose else
    pure <| prose.push (← ``(Verso.Doc.Block.other Block.formalization #[$lean,*]))
  ``(Verso.Doc.Block.other
      (Block.environment $(quote display) $(quote base.toString) $(quote number)
        $(quote marks.label) $(quote marks.title) $(quote marks.of))
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
