import VersoManual

/-!
# Cross-references on the site

Environments and references register themselves as link targets while Verso
traverses the site, in domains of their own, so that proofs, citations, the
bibliography and the index of central environments link to them on whatever
page they are.
-/

namespace Stemma

open Lean
open Verso Doc
open Verso.Genre Manual
open Verso.Multi

/-- The domain of labelled environments, by label. -/
def environmentDomain : Name := `Stemma.environment

/-- The domain of the entries of `references.bib`, by key. -/
def referenceDomain : Name := `Stemma.reference

/-- The name of the block of an environment, which citations inside it look for. -/
def environmentBlockName : Name := `Stemma.Block.environment

/-- What the site knows of a labelled environment: enough to name it and link to it. -/
structure EnvTarget where
  display : String
  base : String
  number : Option Nat
  title : Option String
  central : Bool
  /-- The title of the page it is on. -/
  page : String
  deriving ToJson, FromJson, Inhabited

/-- How readers name an environment: "Theorem 8". -/
def EnvTarget.name (t : EnvTarget) : String :=
  match t.number with
  | some n => s!"{t.display} {n}"
  | none => t.display

/--
The link to an environment's anchor, on its own page. Its anchor is its label,
so that `…/page/#<label>` works; Verso's tag for it only finds its page.
-/
def envLink (link : Link) (label : String) : String :=
  link.path.relativeLink (htmlId := some label)

/-- The page and description of a labelled environment, once the site is traversed. -/
def envTarget? (st : TraverseState) (label : String) : Option (String × EnvTarget) := do
  let link ← (st.resolveDomainObject environmentDomain label).toOption
  let obj ← st.getDomainObject? environmentDomain label
  let target ← (fromJson? obj.data : Except String EnvTarget).toOption
  return (envLink link label, target)

/-- The label of the innermost environment around the current point of the traversal. -/
def environmentLabel? (ctxt : TraverseContext) : Option String :=
  ctxt.blockContext.reverse.findSome? fun
    | .other b =>
      if b.name == environmentBlockName then (b.data.getObjValAs? String "label").toOption
      else none
    | _ => none

/-- The traversal state that lists, for each key, the labels of the environments citing it. -/
def citedByState : Name := `Stemma.citedBy

/-- The traversal state that lists the labels of central environments, in order. -/
def centralState : Name := `Stemma.central

/-- The labels of the environments citing each key. -/
def citedBy (st : TraverseState) : Std.HashMap String (Array String) :=
  match st.get? (α := Array (String × Array String)) citedByState with
  | some (.ok xs) => Std.HashMap.ofList xs.toList
  | _ => {}

/-- Records that the environment `label` cites `key`. -/
def addCitation (st : TraverseState) (key label : String) : TraverseState :=
  let xs := match st.get? (α := Array (String × Array String)) citedByState with
    | some (.ok xs) => xs
    | _ => #[]
  let xs := match xs.findIdx? (·.1 == key) with
    | some i =>
      if xs[i]!.2.contains label then xs
      else xs.modify i fun (k, ls) => (k, ls.push label)
    | none => xs.push (key, #[label])
  st.set citedByState xs

/-- The labels of the central environments, in the order of the site. -/
def centralLabels (st : TraverseState) : Array String :=
  match st.get? (α := Array String) centralState with
  | some (.ok xs) => xs
  | _ => #[]

/-- Records a central environment. -/
def addCentral (st : TraverseState) (label : String) : TraverseState :=
  let xs := centralLabels st
  if xs.contains label then st else st.set centralState (xs.push label)

/-- The title of the page the traversal is on: the document, under the front page. -/
def pageTitle (ctxt : TraverseContext) : String :=
  (ctxt.headers[1]? <|> ctxt.headers[0]?).map (fun (h : PartHeader) => h.titleString) |>.getD ""

/--
The script of the site's environments: buttons that copy a label, and the
signature state of central environments, read from `stemma-state.json` when the
site has one (`stemma preview` writes it). Without it, pages show no state.
-/
def environmentJs : String := r#"
document.addEventListener("DOMContentLoaded", () => {
  for (const button of document.querySelectorAll("button.stemma-copy")) {
    button.addEventListener("click", (event) => {
      event.preventDefault();
      const label = button.dataset.label;
      const done = () => {
        button.classList.add("stemma-copied");
        setTimeout(() => button.classList.remove("stemma-copied"), 1200);
      };
      const fallback = () => {
        const area = document.createElement("textarea");
        area.value = label;
        document.body.appendChild(area);
        area.select();
        try { document.execCommand("copy"); done(); } catch (e) {}
        area.remove();
      };
      if (navigator.clipboard && window.isSecureContext) {
        navigator.clipboard.writeText(label).then(done, fallback);
      } else {
        fallback();
      }
    });
  }
  const marks = document.querySelectorAll("[data-stemma-signature]");
  if (marks.length === 0) return;
  const said = {
    signed: "Signed: a person approved this statement as it is.",
    unsigned: "Unsigned: central, and not signed yet.",
    stale: "Stale: it changed since it was signed.",
  };
  const intents = { signed: "success", unsigned: "warning", stale: "danger" };
  fetch("stemma-state.json")
    .then((response) => (response.ok ? response.json() : null))
    .then((state) => {
      if (!state || !state.environments) return;
      for (const mark of marks) {
        const value = state.environments[mark.dataset.stemmaSignature];
        if (!(value in said)) continue;
        mark.textContent = value;
        mark.title = said[value];
        mark.classList.add("chip", "chip--" + intents[value], "stemma-state-" + value);
      }
    })
    .catch(() => {});
});
"#

/--
The style of what Stemma adds to the site: its layout only. Colours, faces and sizes are the
tokens of `stemma-math/design`, whose theme every page loads (`Stemma.designConfig`), and
what can be a component of it is one, chosen in the HTML: a central environment is a
`.highlight`, a signature state a `.chip` in an intent, the formalization a `.disclosure`
and the index of central environments a `.table` in a `.card`.
-/
def stemmaCss : String := r#"
.stemma-env { margin: var(--space-5) 0; position: relative; }
.stemma-env-heading { margin-bottom: var(--space-1); }
.stemma-statement > p, .stemma-statement > ul, .stemma-statement > ol { font-style: italic; }
.stemma-statement > .stemma-env-heading { font-style: normal; }
.stemma-proof > p:last-of-type::after { content: "∎"; float: right; }
.stemma-env:target { outline: 2px solid var(--focus-ring); outline-offset: var(--space-2);
  border-radius: var(--radius-xs); }
.stemma-formalization { margin-top: var(--space-2); --disclosure-inset: 0; }
.stemma-formalization > summary { font: var(--text-small); color: var(--ink-3); }

/* References to environments and entries, in the link's colour, underlined when hovered. */
.stemma-proof-of a, .stemma-cite a, .stemma-cited a { color: var(--link); text-decoration: none; }
.stemma-proof-of a:hover, .stemma-cite a:hover, .stemma-cited a:hover { text-decoration: underline; }

/* Labels: beside the heading, clearer on hover. */
.stemma-env-meta { float: right; margin-left: var(--space-3); font: var(--text-label);
  display: inline-flex; align-items: center; gap: var(--space-2); }
.stemma-label { color: var(--ink-3); opacity: 0.6; transition: opacity var(--dur-ui) var(--ease-out);
  display: inline-flex; align-items: center; gap: var(--space-1); }
.stemma-env:hover .stemma-label, .stemma-label:focus-within { opacity: 1; }
.stemma-label a { color: inherit; text-decoration: none; }
.stemma-label a:hover { text-decoration: underline; }
/* Missing in stemma-math/design: a copy button as small as the text around it, without the
   elements, which the site does not load. Until then, a quiet text button drawn here. */
button.stemma-copy { font: inherit; color: inherit; background: none; border: 1px solid transparent;
  border-radius: var(--radius-xs); padding: 0 var(--space-1); cursor: pointer; }
button.stemma-copy:hover { border-color: var(--border-strong); color: var(--ink-1); }
button.stemma-copy::after { content: "copy"; }
button.stemma-copy.stemma-copied::after { content: "copied"; }

/* Central environments: a highlight in the accent, a mark, and their signature state. */
.stemma-central-mark { color: var(--accent-text); cursor: help; }
.stemma-signature:empty { display: none; }
.stemma-signature { cursor: help; }

/* Citations and the bibliography. */
.stemma-cited { font-style: normal; color: var(--ink-3); }
.stemma-reference { margin: var(--space-4) 0; padding-left: var(--space-6); text-indent: calc(-1 * var(--space-6)); }
.stemma-reference:target { background: color-mix(in srgb, var(--accent) var(--mix-soft), transparent);
  border-radius: var(--radius-xs); }
.stemma-reference-key { color: var(--ink-3); margin-left: var(--space-2); }
.stemma-cited-by { text-indent: 0; font: var(--text-meta); color: var(--ink-3); margin-top: var(--space-1); }
.stemma-cited-by a { color: inherit; }

/* The index of central environments: one card for each page. */
.stemma-central-index { margin: var(--space-3) 0 var(--space-6); }
.stemma-central-index .table { --columns: minmax(0, 3fr) minmax(0, 2fr) minmax(0, 1fr); }
.stemma-central-index [role="cell"]:first-child a { color: var(--link); }
.stemma-central-index .stemma-label { opacity: 1; }
"#

/--
Missing in stemma-math/design: the Verso theme leaves some colours of the manual genre's own
stylesheet (`book.css`, Verso v4.34.0) as Verso writes them, for a light page: the header, the
table of contents' links and the buttons to the previous and next pages. Until the theme maps
them, they take the tokens here, so that the dark appearance holds together.
-/
def versoStopgapCss : String := r#"
header { background: var(--surface-page); }
.header-title, .prev-next-buttons > * { color: var(--ink-1); }
#toc a { color: var(--ink-2); }
#toc a:hover { color: var(--ink-1); }
#toc .split-toc label.toggle-split-toc::before { background-color: var(--ink-1); }
html:root {
  --verso-burger-toc-visible-shadow-color: var(--surface-page);
  --verso-burger-toc-hidden-color: var(--ink-1);
  --verso-burger-toc-hidden-shadow-color: var(--surface-page);
}
"#

end Stemma
