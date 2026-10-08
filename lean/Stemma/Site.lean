import VersoManual
import Stemma.Genre
import Stemma.Xref
import Stemma.Bibliography
import Stemma.Environment.Directive

/-!
# The library's site

The site is a Verso manual. Its front page is the root module's document, and
its chapters are the library's documents, in the order of the table of
contents, followed by the index of central environments and the bibliography.
`stemma preview` writes the small program that assembles it, so that no list of
chapters is ever maintained by hand.
-/

namespace Stemma

open Lean
open Verso Doc
open Verso.Genre Manual

/-- A front page with only a title, for a library whose root module is not a document. -/
def titlePage (title : String) : Part Manual :=
  { title := #[.text title], titleString := title, metadata := none, content := #[],
    subParts := #[] }

/-- The library's book: its front page, with the documents as chapters. -/
def book (front : Part Manual) (documents : Array (Part Manual)) : Part Manual :=
  { front with subParts := front.subParts ++ documents }

-- The index of central environments, grouped by page, with their signature state.
block_extension Block.centralIndex where
  traverse _ _ _ := pure none
  extraCss := [stemmaCss]
  extraJs := [environmentJs]
  toTeX := some fun _ _ _ _ _ => pure .empty
  toHtml :=
    open Verso.Output.Html in
    some fun _ _ _ _ _ => do
      let st ← Doc.Html.HtmlT.state
      let targets := (centralLabels st).filterMap fun l => (envTarget? st l).map (l, ·)
      if targets.isEmpty then
        return {{<p>"No environment is central yet."</p>}}
      -- Pages in the order of the site, each with its central environments.
      let mut pages : Array (String × Array (String × String × EnvTarget)) := #[]
      for (label, href, t) in targets do
        match pages.findIdx? (·.1 == t.page) with
        | some i => pages := pages.modify i fun (p, xs) => (p, xs.push (label, href, t))
        | none => pages := pages.push (t.page, #[(label, href, t)])
      let mut out : Array Verso.Output.Html := #[]
      for (page, xs) in pages do
        let rows := xs.map fun (label, href, t) =>
          let title : Verso.Output.Html := match t.title with
            | some s => {{" (" {{Verso.Output.Html.text true s}} ")"}}
            | none => .empty
          {{<tr>
              <td><a href={{href}}>{{Verso.Output.Html.text true t.name}}</a>{{title}}</td>
              <td>{{labelHtml label href}}</td>
              <td><span class="stemma-signature" data-stemma-signature={{label}}></span></td>
            </tr>}}
        out := out.push {{
          <table class="stemma-central-index">
            <thead><tr><th>{{page}}</th><th>"Label"</th><th>"Signature"</th></tr></thead>
            <tbody>{{rows}}</tbody>
          </table>}}
      return .seq out

/-- The page that lists the central environments. -/
def centralPart : Part Manual :=
  { title := #[.text "Central environments"], titleString := "Central environments",
    metadata := some { file := some "central", number := false },
    content := #[
      .para #[.text "The central environments are what the library claims: a person signs \
        each of them. A signature is signed while the environment is as it was signed, \
        unsigned when it was never signed, and stale when it changed since."],
      .other Block.centralIndex #[]],
    subParts := #[] }

/-- The whole site: the book, then the index of central environments and the bibliography. -/
def site (front : Part Manual) (documents : Array (Part Manual))
    (references : Array BibTeX.Entry) : Part Manual :=
  book front (documents ++ #[centralPart, bibliographyPart references])

/--
The site's program: reads `references.bib` from the library's directory, where
`stemma preview` runs it, and builds the site with Verso. A malformed
`references.bib` stops it with an error that says what is wrong.
-/
def siteMain (args : List String) (front : Part Manual) (documents : Array (Part Manual))
    (extensionImpls : ExtensionImpls := by exact extension_impls%) : IO UInt32 := do
  match ← readReferences "." with
  | .error e =>
    IO.eprintln s!"error: {e}"
    return 1
  | .ok references =>
    manualMain (site front documents references) (extensionImpls := extensionImpls)
      (options := args)

end Stemma
