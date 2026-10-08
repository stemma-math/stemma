import VersoManual
import Stemma.BibTeX
import Stemma.Xref

/-!
# References

`references.bib`, at the root of the library, is the source of truth of its
references. Documents cite its entries with the `cite` role, and environments
with their `cited` mark; both are checked against it when the document is
elaborated, the way `{name}` is checked against Lean. The site lists every
entry on a page of its own, with the environments that cite it.
-/

namespace Stemma

open Lean Elab
open Verso Doc Elab ArgParse
open Verso.Genre Manual
open Verso.Multi

/-- The file of a library's references, at its root. -/
def referencesFile : String := "references.bib"

/--
The root of the Lake package a source file belongs to: the nearest directory
above it with a `lakefile.toml` or a `lakefile.lean`.
-/
partial def packageRoot? (file : System.FilePath) : IO (Option System.FilePath) := do
  let file ← try IO.FS.realPath file catch _ => pure file
  let rec up (dir : Option System.FilePath) : IO (Option System.FilePath) := do
    let some d := dir | return none
    if (← (d / "lakefile.toml").pathExists) || (← (d / "lakefile.lean").pathExists) then
      return some d
    -- `parent` of a root is itself, or none.
    match d.parent with
    | some p => if p == d then return none else up (some p)
    | none => return none
  up file.parent

/--
Reads the references of a library from `references.bib` at `root`. A missing
file has no entries; a malformed one is an error that says what is wrong.
-/
def readReferences (root : System.FilePath) : IO (Except String (Array BibTeX.Entry)) := do
  let path := root / referencesFile
  unless ← path.pathExists do return .ok #[]
  let text ← IO.FS.readFile path
  return (BibTeX.parse text).mapError (s!"{referencesFile} is malformed, {·}")

/-- The entry `key` of the library's references, or an error saying why there is none. -/
def findReference (key : String) : DocElabM BibTeX.Entry := do
  let some root ← packageRoot? (← getFileName)
    | throwError m!"Cannot find the library's root, where {referencesFile} is, to cite '{key}'."
  match ← readReferences root with
  | .error e => throwError e
  | .ok entries =>
    let some e := entries.find? (·.key == key)
      | throwError m!"The reference '{key}' is not in {referencesFile}. Add it there, with its \
          author, title and year, before citing it."
    return e

open Verso.Output.Html in
/-- How a citation is shown, once the bibliography is known. -/
def citationHtml (key : String) (locator : Option String) :
    Doc.Html.HtmlT Manual (ReaderT AllRemotes (ReaderT ExtensionImpls (BuildLogT IO))) Output.Html := do
  let st ← Doc.Html.HtmlT.state
  let loc : Output.Html := match locator with
    | some l => {{", " {{l}}}}
    | none => .empty
  let found := do
    let link ← (st.resolveDomainObject referenceDomain key).toOption
    let obj ← st.getDomainObject? referenceDomain key
    let entry ← (fromJson? obj.data : Except String BibTeX.Entry).toOption
    pure (link, entry)
  match found with
  | some (link, entry) =>
    pure {{"["<a href={{link.relativeLink}} title={{entry.reference}}>{{entry.short}}</a>{{loc}}"]"}}
  | none =>
    reportError s!"The reference '{key}' is not in {referencesFile}."
    pure {{"[" {{key}} {{loc}} "]"}}

-- A citation of an entry of `references.bib`, with an optional locator.
inline_extension Inline.cite (key : String) (locator : Option String) where
  data := json%{"key": $key, "locator": $locator}
  traverse _ data _ := do
    let .ok key := data.getObjValAs? String "key" | return none
    if let some label := environmentLabel? (← read) then
      modify (addCitation · key label)
    return none
  extraCss := [stemmaCss]
  extraJs := [environmentJs]
  toTeX := some fun go _ _ content => content.mapM go
  toHtml :=
    open Verso.Output.Html in
    some fun _ _ data _ => do
      let .ok key := data.getObjValAs? String "key" | pure .empty
      let locator := (data.getObjValAs? String "locator").toOption
      pure {{<span class="stemma-cite">{{← citationHtml key locator}}</span>}}

/-- The arguments of `cite`: an optional locator, such as "Def. II.1.3". -/
structure CiteArgs where
  locator : Option String

instance : FromArgs CiteArgs DocElabM where
  fromArgs := CiteArgs.mk <$> ((some <$> .positional `locator .string) <|> pure none) <* .done

/--
Cites an entry of `references.bib` by its key, with an optional locator:
``{cite}`BurrisSankappanavar` `` or ``{cite "Def. II.1.3"}`BurrisSankappanavar` ``.
The key must be in `references.bib`.
-/
@[role]
def cite : RoleExpanderOf CiteArgs
  | {locator}, content => do
    let key ← oneCodeStr content
    let k := key.getString.trimAscii.toString
    discard <| withRef key (findReference k)
    ``(Verso.Doc.Inline.other (Inline.cite $(quote k) $(quote locator)) #[])

-- An entry of the bibliography, with the environments that cite it.
block_extension Block.reference (entry : BibTeX.Entry) where
  data := toJson entry
  traverse id data _ := do
    let .ok (entry : BibTeX.Entry) := fromJson? data | return none
    let path := (← read).path
    discard <| externalTag id path s!"ref-{entry.key}"
    modify fun st => st.saveDomainObject referenceDomain entry.key id
      |>.saveDomainObjectData referenceDomain entry.key data
    return none
  extraCss := [stemmaCss]
  toTeX := some fun _ goB _ _ content => content.mapM goB
  toHtml :=
    open Verso.Output.Html in
    some fun _ _ id data _ => do
      let .ok (e : BibTeX.Entry) := fromJson? data | pure .empty
      let st ← Doc.Html.HtmlT.state
      let title : Output.Html := match e.url with
        | some u => {{<a href={{u}}>{{BibTeX.sentence e.title}}</a>}}
        | none => .text true (BibTeX.sentence e.title)
      let venue : Output.Html := match e.venue with
        | some v => {{" " {{BibTeX.sentence v}}}}
        | none => .empty
      let citing := ((citedBy st).get? e.key).getD #[] |>.filterMap fun label =>
        (envTarget? st label).map fun (href, t) => {{<a href={{href}}>{{t.name}}</a>}}
      let citedBy : Output.Html :=
        if citing.isEmpty then .empty
        else
          let items := citing.toList.intersperse {{", "}}
          {{<div class="stemma-cited-by">"Cited by " {{items.toArray}} "."</div>}}
      pure {{
        <div class="stemma-reference" {{st.htmlId id}}>
          {{e.people}} " (" {{e.year}} "). " <em>{{title}}</em> {{venue}}
          <span class="stemma-reference-key">{{e.key}}</span>
          {{citedBy}}
        </div>
      }}

/-- The site's page of references: every entry of `references.bib`, in order. -/
def bibliographyPart (entries : Array BibTeX.Entry) : Part Manual :=
  let sorted := entries.qsort fun a b => a.sortKey < b.sortKey
  let content : Array (Block Manual) :=
    if sorted.isEmpty then
      #[.para #[.text s!"The library's {referencesFile} has no entries yet."]]
    else sorted.map fun e => .other (Block.reference e) #[]
  { title := #[.text "Bibliography"], titleString := "Bibliography",
    metadata := some { file := some "bibliography", number := false },
    content, subParts := #[] }

end Stemma
