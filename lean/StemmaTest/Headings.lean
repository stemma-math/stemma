import StemmaTest.Citations

/-!
The site shows what elaboration decided: which proofs name what they prove,
what environments and prose cite, and the headings of proofs.
-/

open Lean Verso Doc Verso.Genre Stemma

/-- The data of every environment of a document, in order. -/
partial def environments (b : Block Manual) : Array EnvData :=
  match b with
  | .other c bs =>
    let here := if c.name == ``Stemma.Block.environment then
      (fromJson? c.data |>.toOption).toArray else #[]
    here ++ bs.flatMap environments
  | .ul items | .ol _ items => items.flatMap (·.contents.flatMap environments)
  | .blockquote bs | .concat bs => bs.flatMap environments
  | _ => #[]

/-- The citations of a document's prose: key and locator. -/
partial def citations (i : Verso.Doc.Inline Manual) : Array (String × Option String) :=
  match i with
  | .other c is =>
    let here := if c.name == ``Stemma.Inline.cite then
      match c.data.getObjValAs? String "key" with
      | .ok k => #[(k, (c.data.getObjValAs? String "locator").toOption)]
      | .error _ => #[]
      else #[]
    here ++ is.flatMap citations
  | .emph is | .bold is | .concat is | .link is _ => is.flatMap citations
  | _ => #[]

partial def blockCitations (b : Block Manual) : Array (String × Option String) :=
  match b with
  | .para is => is.flatMap citations
  | .other _ bs | .blockquote bs | .concat bs => bs.flatMap blockCitations
  | _ => #[]

def doc : Part Manual := (%doc StemmaTest.Citations : Part _root_.Stemma)

def envs : Array EnvData := doc.content.flatMap environments

/-- info: #[("algebra", false), ("algebra", true), ("algebra-trivial", false), ("algebra-trivial", false), ("even-add", false)] -/
#guard_msgs in
#eval envs.filterMap fun d => (d.label <|> d.of).map (·, d.adjacent)

/-- info: [some ("BurrisSankappanavar", some "Def. II.1.3")] -/
#guard_msgs in
#eval envs.toList.filter (·.label == some "algebra") |>.map fun d => d.cited.map (·, d.locator)

/--
info: #[("BurrisSankappanavar", none), ("BurrisSankappanavar", some "Def. II.1.3"), ("Folklore", none)]
-/
#guard_msgs in
#eval doc.content.flatMap blockCitations

-- Proof headings: "Proof." when adjacent, "Proof of Theorem 2." with a link otherwise.
/--
info: [("Proof", none), ("Proof of ", some ("Theorem 2", "Citations-and-proofs/#algebra-trivial")),
  ("Proof of ", some ("Theorem 2", "Citations-and-proofs/#algebra-trivial"))]
-/
#guard_msgs in
#eval envs.toList.filter (·.base == "proof") |>.map fun d =>
  proofHeading d (some ("Theorem 2", "Citations-and-proofs/#algebra-trivial"))

/-- info: ("Proof of even-add", none) -/
#guard_msgs in
#eval proofHeading { display := "Proof", base := "proof", of := some "even-add" } none

-- The proof in the module of its statement, right after it, does not name it.
/-- info: #[true] -/
#guard_msgs in
#eval (((%doc StemmaTest.Even : Part _root_.Stemma) : Part Manual).content.flatMap environments).filterMap fun d =>
  if d.base == "proof" then some d.adjacent else none
