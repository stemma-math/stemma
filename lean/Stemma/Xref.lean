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
  fetch("stemma-state.json")
    .then((response) => (response.ok ? response.json() : null))
    .then((state) => {
      if (!state || !state.environments) return;
      for (const mark of marks) {
        const value = state.environments[mark.dataset.stemmaSignature];
        if (!(value in said)) continue;
        mark.textContent = value;
        mark.title = said[value];
        mark.classList.add("stemma-state-" + value);
      }
    })
    .catch(() => {});
});
"#

/--
The style of what Stemma adds to the site. Colours derive from the text's, or
are mid-tones readable on light and dark backgrounds alike.
-/
def stemmaCss : String := r#"
:root {
  --stemma-muted: color-mix(in srgb, currentColor 55%, transparent);
  --stemma-faint: color-mix(in srgb, currentColor 18%, transparent);
  --stemma-accent: #3f7fbf;
  --stemma-signed: #2e9d5b;
  --stemma-unsigned: #b7841a;
  --stemma-stale: #d1493f;
}
.stemma-env { margin: 1.2em 0; position: relative; }
.stemma-env-heading { margin-bottom: 0.3em; }
.stemma-statement > p, .stemma-statement > ul, .stemma-statement > ol { font-style: italic; }
.stemma-statement > .stemma-env-heading { font-style: normal; }
.stemma-proof > p:last-of-type::after { content: "∎"; float: right; }
.stemma-proof-of a { color: inherit; text-decoration: underline dotted; text-underline-offset: 0.15em; }
.stemma-proof-of a:hover { text-decoration-style: solid; }
.stemma-formalization { margin-top: 0.5em; border-left: 3px solid var(--stemma-faint); padding-left: 0.8em; }
.stemma-formalization > summary { cursor: pointer; font-size: 0.85em; color: var(--stemma-muted); }
.stemma-env:target { outline: 2px solid var(--stemma-faint); outline-offset: 0.4em; border-radius: 2px; }

/* Labels: small, beside the heading, clearer on hover. */
.stemma-env-meta { float: right; margin-left: 0.8em; font-size: 0.75em; font-style: normal;
  display: inline-flex; align-items: center; gap: 0.45em; }
.stemma-label { font-family: var(--verso-code-font-family, monospace); color: var(--stemma-muted);
  opacity: 0.55; transition: opacity 0.15s; display: inline-flex; align-items: center; gap: 0.2em; }
.stemma-env:hover .stemma-label, .stemma-label:focus-within { opacity: 1; }
.stemma-label a { color: inherit; text-decoration: none; }
.stemma-label a:hover { text-decoration: underline; }
button.stemma-copy { font: inherit; color: inherit; background: none; border: 1px solid transparent;
  border-radius: 3px; padding: 0 0.25em; cursor: pointer; line-height: 1.3; }
button.stemma-copy:hover { border-color: var(--stemma-faint); }
button.stemma-copy::after { content: "copy"; }
button.stemma-copy.stemma-copied::after { content: "copied"; }

/* Central environments: a thin rule in the margin, a mark, and their signature state. */
.stemma-central::before { content: ""; position: absolute; left: -0.9em; top: 0.15em; bottom: 0.15em;
  width: 3px; border-radius: 2px; background: var(--stemma-accent); opacity: 0.8; }
.stemma-central-mark { color: var(--stemma-accent); font-size: 0.9em; cursor: help; }
.stemma-signature:empty { display: none; }
.stemma-signature { border: 1px solid currentColor; border-radius: 0.7em; padding: 0 0.5em;
  font-size: 0.95em; line-height: 1.4; cursor: help; }
.stemma-state-signed { color: var(--stemma-signed); background: color-mix(in srgb, var(--stemma-signed) 12%, transparent); }
.stemma-state-unsigned { color: var(--stemma-unsigned); background: color-mix(in srgb, var(--stemma-unsigned) 12%, transparent); }
.stemma-state-stale { color: var(--stemma-stale); background: color-mix(in srgb, var(--stemma-stale) 12%, transparent); }

/* Citations and the bibliography. */
.stemma-cite a, .stemma-cited a { color: inherit; text-decoration: underline dotted; text-underline-offset: 0.15em; }
.stemma-cite a:hover, .stemma-cited a:hover { text-decoration-style: solid; }
.stemma-cited { font-style: normal; color: var(--stemma-muted); }
.stemma-reference { margin: 0.9em 0; padding-left: 1.5em; text-indent: -1.5em; }
.stemma-reference:target { background: color-mix(in srgb, var(--stemma-accent) 12%, transparent); }
.stemma-reference-key { font-family: var(--verso-code-font-family, monospace); font-size: 0.8em;
  color: var(--stemma-muted); margin-left: 0.5em; }
.stemma-cited-by { text-indent: 0; font-size: 0.9em; color: var(--stemma-muted); margin-top: 0.15em; }
.stemma-cited-by a { color: inherit; }

/* The index of central environments. */
table.stemma-central-index { border-collapse: collapse; width: 100%; margin: 0.5em 0 1.5em; }
table.stemma-central-index td, table.stemma-central-index th { text-align: left; padding: 0.3em 0.6em 0.3em 0;
  border-bottom: 1px solid var(--stemma-faint); vertical-align: baseline; }
table.stemma-central-index th { font-weight: 600; font-size: 0.85em; color: var(--stemma-muted); }
table.stemma-central-index .stemma-label { opacity: 1; }
table.stemma-central-index .stemma-signature { font-size: 0.75em; }
"#

end Stemma
