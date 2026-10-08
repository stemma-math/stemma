import VersoManual
import Stemma.Environment.Record
import Stemma.Environment.Obligations
import Stemma.Bibliography

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

/-- What the site shows of an environment, which elaboration decides. -/
structure EnvData where
  display : String
  base : String
  /-- Definitions, statements and remarks are numbered in their module. -/
  number : Option Nat := none
  label : Option String := none
  title : Option String := none
  central : Bool := false
  /-- The key of the entry of `references.bib` a `cited` environment rests on. -/
  cited : Option String := none
  /-- Where in the cited work, after the key in the `cited` mark. -/
  locator : Option String := none
  /-- For a proof, the label of what it proves. -/
  of : Option String := none
  /-- Whether a proof immediately follows what it proves, and so need not name it. -/
  adjacent : Bool := false
  deriving ToJson, FromJson

/-- How readers name an environment: "Theorem 8". -/
def EnvData.name (d : EnvData) : String :=
  match d.number with
  | some n => s!"{d.display} {n}"
  | none => d.display

/--
The heading of a proof, as text and an optional link target: "Proof." when it
immediately follows what it proves, and "Proof of Theorem 8." otherwise, with
"Theorem 8" linked. `target` is the name of what it proves, and its link.
-/
def proofHeading (d : EnvData) (target : Option (String × String)) :
    String × Option (String × String) :=
  if d.adjacent || d.of.isNone then (d.display, none)
  else match target with
    | some t => (s!"{d.display} of ", some t)
    | none => (s!"{d.display} of {d.of.getD ""}", none)

open Verso.Output.Html in
/-- An environment's label, discreet, linked to its anchor, with a button that copies it. -/
def labelHtml (label : String) (href : String) : Verso.Output.Html :=
  {{<span class="stemma-label">
      <a href={{href}} title="The environment's label">{{label}}</a>
      <button class="stemma-copy" type="button" data-label={{label}} title="Copy the label"></button>
    </span>}}

open Verso.Output.Html in
/-- The mark of a central environment, and the place its signature state goes. -/
def centralHtml (label : String) : Verso.Output.Html :=
  {{<span class="stemma-central-mark"
        title="Central: part of what the library claims, signed by a person">"◆"</span>
    <span class="stemma-signature" data-stemma-signature={{label}}></span>}}

-- A mathematical environment.
block_extension Block.environment (d : EnvData) where
  data := toJson d
  traverse id data _ := do
    let .ok (d : EnvData) := fromJson? data | return none
    let some label := d.label | return none
    let ctxt ← read
    -- Verso's tag finds the page; the anchor on it is the label itself.
    discard <| externalTag id ctxt.path label
    let target : EnvTarget := {
      display := d.display, base := d.base, number := d.number, title := d.title
      central := d.central, page := pageTitle ctxt }
    modify fun st => st.saveDomainObject environmentDomain label id
      |>.saveDomainObjectData environmentDomain label (toJson target)
    if d.central then modify (addCentral · label)
    if let some key := d.cited then modify (addCitation · key label)
    return none
  extraCss := [stemmaCss]
  extraJs := [environmentJs]
  toTeX := some fun _ goB _ _ content => content.mapM goB
  toHtml :=
    open Verso.Output.Html in
    some fun _ goB _ data content => do
      let .ok (d : EnvData) := fromJson? data | pure .empty
      let st ← Doc.Html.HtmlT.state
      let title : Verso.Output.Html := match d.title with
        | some t => {{" (" {{t}} ")"}}
        | none => .empty
      let heading ← if d.base == "proof" then do
          let target := d.of.bind fun l => (envTarget? st l).map fun (href, t) => (t.name, href)
          if d.of.isSome && !d.adjacent && target.isNone then
            reportError s!"The proof of '{d.of.getD ""}' names an environment the site does not show."
          let (words, link) := proofHeading d target
          let link : Verso.Output.Html := match link with
            | some (name, href) => {{<a href={{href}} title={{d.of.getD ""}}>{{name}}</a>}}
            | none => .empty
          pure {{<em class="stemma-proof-of">{{words}}{{link}}{{title}}"."</em>}}
        else pure {{<strong>{{d.name}}</strong>{{title}}"."}}
      let cited : Verso.Output.Html ← match d.cited with
        | some key => do
          pure {{" "<span class="stemma-cited">{{← citationHtml key d.locator}}</span>}}
        | none => pure .empty
      let side : Verso.Output.Html := match d.label with
        | some label =>
          let href := (envTarget? st label).map (·.1) |>.getD s!"#{label}"
          let central := if d.central then centralHtml label else .empty
          {{<span class="stemma-env-meta">{{central}}{{labelHtml label href}}</span>}}
        | none => .empty
      let classes := s!"stemma-env stemma-{d.base}" ++ (if d.central then " stemma-central" else "")
      let attrs : Array (String × String) := match d.label with
        | some label => #[("id", label), ("data-stemma-label", label)]
        | none => #[]
      pure {{
        <div class={{classes}} {{attrs}}>
          <p class="stemma-env-heading">{{side}}{{heading}}{{cited}}</p>
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

/--
Whether a declaration is one a person wrote, as Lean itself tells: not
internal, and not a recursor, constructor, projection, matcher or other
auxiliary declaration Lean makes for one. Lemmas Lean derives for a type and
records under the type's name (such as `injEq`) are kept: they are part of what
the environment adds, and Lean marks them no differently.
-/
def isUserDecl (env : Environment) (n : Name) : Bool :=
  let generatedKind := match env.find? n with
    | some (.ctorInfo _) | some (.recInfo _) => true
    | _ => false
  !n.isInternal && !generatedKind && !isAuxRecursor env n && !isNoConfusion env n &&
    !isMatcherCore env n && !(env.isProjectionFn n)

/-- Recover the source span of composite blocks, including lists whose wrapper
has no source information of its own. -/
private partial def sourceBounds (stx : Syntax) : Option (String.Pos.Raw × String.Pos.Raw) := Id.run do
  let mut bounds := stx.getPos?.bind fun s => stx.getTailPos?.map fun e => (s, e)
  for child in stx.getArgs do
    if let some (s, e) := sourceBounds child then
      bounds := some <| match bounds with
        | none => (s, e)
        | some (lo, hi) =>
          (if s.byteIdx < lo.byteIdx then s else lo,
           if hi.byteIdx < e.byteIdx then e else hi)
  return bounds

/--
The source of an environment's blocks: those of Lean code when `lean` is true,
and the others, its prose, when it is false.
-/
def sourceOf (contents : TSyntaxArray `block) (lean : Bool) : DocElabM String := do
  let text ← getFileMap
  let mut parts := #[]
  for b in contents do
    if (b.raw.getKind == ``Lean.Doc.Syntax.codeblock) != lean then continue
    if let some (s, e) := sourceBounds b.raw then
      parts := parts.push (String.Pos.Raw.extract text.source s e).trimAscii.toString
  return "\n\n".intercalate parts.toList

/--
Where the last environment of the current module ends, and its label: a proof
that starts right after the environment it proves need not name it.
-/
initialize lastEnvironmentExt : EnvExtension (Option (Option String × String.Pos.Raw)) ←
  registerEnvExtension (pure none)

/--
Whether a proof of `target` starting at `start` immediately follows it: the
last environment is `target`, and only blank space lies between them.
-/
def followsDirectly (target : String) (start : String.Pos.Raw) : DocElabM Bool := do
  let some (some label, stop) := lastEnvironmentExt.getState (← getEnv) | return false
  if label != target || start.byteIdx < stop.byteIdx then return false
  let between := String.Pos.Raw.extract (← getFileMap).source stop start
  return between.all Char.isWhitespace

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
  if let some cited := marks.cited then
    unless base == .definition || base == .statement do
      throwError "Only definitions and statements can be cited."
    let (key, _) := BibTeX.splitCitation cited
    if key.isEmpty then
      throwError m!"The mark 'cited' names an entry of {referencesFile} by its key, \
        optionally followed by a comma and where in it: \"Key2001, Prop. 3.4\"."
    discard <| findReference key

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
  let adjacent ← match marks.of, ref.getPos? with
    | some target, some start => followsDirectly target start
    | _, _ => pure false
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
  -- In source order; declarations Lean makes at the same place, by name.
  let decls := added.qsort (fun a b => a.1.line < b.1.line ||
    (a.1.line == b.1.line && (a.1.column < b.1.column ||
      (a.1.column == b.1.column && Name.lt a.2 b.2)))) |>.map (·.2)
  checkDecls marks decls
  let fileMap ← getFileMap
  let line := fileMap.toPosition (ref.getPos?.getD 0) |>.line
  let endLine := max line (fileMap.toPosition (ref.getTailPos?.getD 0) |>.line)
  modifyEnv (recordExt.addEntry · {
    name, display, base, decls, line, endLine
    label := marks.label, central := marks.central, cited := marks.cited
    title := marks.title, of := marks.of
    prose := ← sourceOf contents (lean := false)
    lean := ← sourceOf contents (lean := true)
    module := env.mainModule
  })
  if let some stop := ref.getTailPos? then
    modifyEnv (lastEnvironmentExt.setState · (some (marks.label, stop)))
  let blocks ← if lean.isEmpty then pure prose else
    pure <| prose.push (← ``(Verso.Doc.Block.other Block.formalization #[$lean,*]))
  let (cited, locator) := match marks.cited.map BibTeX.splitCitation with
    | some (key, loc) => (some key, loc)
    | none => (none, none)
  `(Verso.Doc.Block.other
      (Stemma.Block.environment ({
        display := $(quote display), base := $(quote base.toString), number := $(quote number)
        label := $(quote marks.label), title := $(quote marks.title)
        central := $(quote marks.central), cited := $(quote cited), locator := $(quote locator)
        of := $(quote marks.of), adjacent := $(quote adjacent) } : Stemma.EnvData))
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
