import Lean

/-!
# A minimal BibTeX reader

A library's references live in `references.bib`, edited by people. Stemma reads
only what it shows: each entry's key, authors (or editors), title, year, and
venue or publisher, with a link when the entry has a `url` or a `doi`.

The reader is strict: a malformed entry, a duplicate key, a missing required
field or text outside every entry is an error that names the line, never a
silent omission. Outside entries only blank space and `%` comments are allowed.
`@comment` and `@preamble` entries are skipped; `@string` abbreviations are not
supported.
-/

namespace Stemma.BibTeX

open Lean

/-- A person, as an entry names them. -/
structure Person where
  /-- The name as shown: given names first. -/
  full : String
  /-- The family name, which citations show. -/
  last : String
  deriving Inhabited, Repr, BEq, ToJson, FromJson

/-- An entry of the bibliography, with the fields Stemma shows. -/
structure Entry where
  key : String
  /-- The entry type, lowercase: `article`, `book`, … -/
  kind : String
  authors : Array Person
  editors : Array Person
  title : String
  year : String
  /-- Where it appeared, or who published it. -/
  venue : Option String
  /-- A link to it, from `url` or `doi`. -/
  url : Option String
  /-- The line where the entry starts. -/
  line : Nat
  deriving Inhabited, Repr, BEq, ToJson, FromJson

/-- The parser's state: the text, as characters, and a position in it. -/
private structure State where
  chars : Array Char
  pos : Nat := 0

private abbrev P := StateT State (Except String)

private def lineAt (chars : Array Char) (pos : Nat) : Nat := Id.run do
  let mut line := 1
  for i in [0:min pos chars.size] do
    if chars[i]! == '\n' then line := line + 1
  return line

private def fail (msg : String) : P α := do
  let s ← get
  throw s!"line {lineAt s.chars s.pos}: {msg}"

private def peek? : P (Option Char) := do
  let s ← get
  return s.chars[s.pos]?

private def advance : P Unit := modify fun s => { s with pos := s.pos + 1 }

private def next : P Char := do
  let some c ← peek? | fail "the file ends inside an entry"
  advance
  return c

private partial def skipSpace : P Unit := do
  if let some c ← peek? then
    if c.isWhitespace then advance; skipSpace

private def expect (c : Char) (what : String) : P Unit := do
  skipSpace
  match ← peek? with
  | some d => if d == c then advance else fail s!"expected {what}, found '{d}'"
  | none => fail s!"expected {what}, but the file ends"

private def isIdentChar (c : Char) : Bool :=
  c.isAlphanum || c == '_' || c == '-' || c == ':' || c == '.' || c == '+' || c == '/'

private partial def identifier (what : String) : P String := do
  skipSpace
  let rec go (acc : String) : P String := do
    match ← peek? with
    | some c => if isIdentChar c then advance; go (acc.push c) else pure acc
    | none => pure acc
  let id ← go ""
  if id.isEmpty then
    match ← peek? with
    | some c => fail s!"expected {what}, found '{c}'"
    | none => fail s!"expected {what}, but the file ends"
  return id

/-- The text up to the brace that closes one already read, braces kept inside. -/
private partial def braced (acc : String := "") (depth : Nat := 0) : P String := do
  let c ← next
  if c == '}' then
    if depth == 0 then return acc else braced (acc.push c) (depth - 1)
  else if c == '{' then braced (acc.push c) (depth + 1)
  else braced (acc.push c) depth

/-- The text up to the quote that closes one already read, outside braces. -/
private partial def quoted (acc : String := "") (depth : Nat := 0) : P String := do
  let c ← next
  if c == '"' && depth == 0 then return acc
  else if c == '{' then quoted (acc.push c) (depth + 1)
  else if c == '}' then
    if depth == 0 then fail "unbalanced '}' in a quoted value" else quoted (acc.push c) (depth - 1)
  else quoted (acc.push c) depth

/-- A field's value: braced or quoted text, or a number, joined with `#`. -/
private partial def value : P String := do
  skipSpace
  let part ← match ← peek? with
    | some '{' => advance; braced
    | some '"' => advance; quoted
    | some c =>
      if c.isDigit then
        let rec digits (acc : String) : P String := do
          match ← peek? with
          | some d => if d.isDigit then advance; digits (acc.push d) else pure acc
          | none => pure acc
        digits ""
      else if c.isAlpha then
        let name ← identifier "a value"
        fail s!"'{name}' is an @string abbreviation, which is not supported: write the value in braces"
      else fail s!"expected a value in braces or quotes, found '{c}'"
    | none => fail "expected a value, but the file ends"
  skipSpace
  if (← peek?) == some '#' then
    advance
    return part ++ (← value)
  return part

/-- Splits `text` at ` and ` outside braces. -/
private def splitNames (text : String) : Array String := Id.run do
  let words := text.toList
  let mut out := #[]
  let mut cur := ""
  let mut depth := 0
  let mut i := 0
  let cs := words.toArray
  while i < cs.size do
    let c := cs[i]!
    if c == '{' then depth := depth + 1
    if c == '}' then depth := depth - 1
    if depth == 0 && c.isWhitespace && i + 4 < cs.size &&
        cs[i+1]!.toLower == 'a' && cs[i+2]!.toLower == 'n' && cs[i+3]!.toLower == 'd' &&
        cs[i+4]!.isWhitespace then
      out := out.push cur
      cur := ""
      i := i + 5
    else
      cur := cur.push c
      i := i + 1
  out := out.push cur
  return out.map (·.trimAscii.toString) |>.filter (!·.isEmpty)

/-- The combining mark a LaTeX accent command adds. -/
private def accent? : Char → Option Char
  | '\'' => some '́'
  | '`' => some '̀'
  | '^' => some '̂'
  | '"' => some '̈'
  | '~' => some '̃'
  | '=' => some '̄'
  | '.' => some '̇'
  | 'c' => some '̧'
  | 'v' => some '̌'
  | 'u' => some '̆'
  | 'H' => some '̋'
  | _ => none

/-- Precomposed letters, as triples of a letter, a combining mark and their composition. -/
private def compositions : String := "áá éé íí óó úú ýý ÁÁ ÉÉ ÍÍ ÓÓ ÚÚ ÝÝ ćć ĆĆ ńń ŃŃ śś ŚŚ źź ŹŹ ǵǵ ǴǴ ŕŕ ŔŔ ĺĺ ĹĹ ḱḱ ḰḰ ẃẃ ẂẂ àà èè ìì òò ùù ỳỳ ÀÀ ÈÈ ÌÌ ÒÒ ÙÙ ỲỲ ǹǹ ǸǸ ẁẁ ẀẀ ââ êê îî ôô ûû ŷŷ ÂÂ ÊÊ ÎÎ ÔÔ ÛÛ ŶŶ ĉĉ ĈĈ ŝŝ ŜŜ ẑẑ ẐẐ ĝĝ ĜĜ ŵŵ ŴŴ ää ëë ïï öö üü ÿÿ ÄÄ ËË ÏÏ ÖÖ ÜÜ ŸŸ ẗẗ ẅẅ ẄẄ ãã ẽẽ ĩĩ õõ ũũ ỹỹ ÃÃ ẼẼ ĨĨ ÕÕ ŨŨ ỸỸ ññ ÑÑ āā ēē īī ōō ūū ȳȳ ĀĀ ĒĒ ĪĪ ŌŌ ŪŪ ȲȲ ḡḡ ḠḠ ȧȧ ėė ȯȯ ẏẏ ȦȦ ĖĖ İİ ȮȮ ẎẎ ċċ ĊĊ ṅṅ ṄṄ ṡṡ ṠṠ żż ŻŻ ġġ ĠĠ ṙṙ ṘṘ ṫṫ ṪṪ ḋḋ ḊḊ ẇẇ ẆẆ ȩȩ ȨȨ çç ÇÇ ņņ ŅŅ şş ŞŞ ģģ ĢĢ ŗŗ ŖŖ ļļ ĻĻ ţţ ŢŢ ḑḑ ḐḐ ķķ ĶĶ ǎǎ ěě ǐǐ ǒǒ ǔǔ ǍǍ ĚĚ ǏǏ ǑǑ ǓǓ čč ČČ ňň ŇŇ šš ŠŠ žž ŽŽ ǧǧ ǦǦ řř ŘŘ ľľ ĽĽ ťť ŤŤ ďď ĎĎ ǩǩ ǨǨ ăă ĕĕ ĭĭ ŏŏ ŭŭ ĂĂ ĔĔ ĬĬ ŎŎ ŬŬ ğğ ĞĞ őő űű ŐŐ ŰŰ"

/-- The precomposed form of a letter with a combining accent, when there is one. -/
private def compose (base mark : Char) : String :=
  let cs := compositions.toList.toArray
  Id.run do
    let mut i := 0
    while i + 2 < cs.size do
      if cs[i]! == base && cs[i+1]! == mark then return (cs[i+2]!).toString
      i := i + 4
    return (base.toString.push mark)

/--
The text a value shows: braces removed, LaTeX accents and a few commands
turned into characters, dashes and ties into their glyphs, spaces collapsed.
Other commands are dropped, keeping their arguments.
-/
def plain (text : String) : String := Id.run do
  let cs := text.toList.toArray
  let mut out := ""
  let mut i := 0
  while i < cs.size do
    let c := cs[i]!
    if c == '\\' && i + 1 < cs.size then
      let d := cs[i+1]!
      if let some mark := accent? d then
        -- `\'e`, `\'{e}` or `{\'e}`; letter accents (`\c`, `\v`, …) need a space or braces.
        let mut j := i + 2
        while j < cs.size && (cs[j]! == '{' || (d.isAlpha && cs[j]! == ' ')) do j := j + 1
        if j < cs.size && cs[j]!.isAlpha then
          out := out ++ compose cs[j]! mark
          j := j + 1
          while j < cs.size && cs[j]! == '}' do j := j + 1
          i := j
          continue
      if d.isAlpha then
        let mut j := i + 1
        let mut name := ""
        while j < cs.size && cs[j]!.isAlpha do
          name := name.push cs[j]!
          j := j + 1
        let replacement := match name with
          | "ss" => "ß" | "o" => "ø" | "O" => "Ø" | "ae" => "æ" | "AE" => "Æ"
          | "oe" => "œ" | "OE" => "Œ" | "aa" => "å" | "AA" => "Å" | "l" => "ł" | "L" => "Ł"
          | "i" => "ı" | "j" => "ȷ" | "TeX" => "TeX" | "LaTeX" => "LaTeX"
          | _ => ""
        out := out ++ replacement
        i := j
        continue
      -- `\&`, `\%`, `\$`, `\_`, `\{`, …: the character itself.
      out := out.push d
      i := i + 2
      continue
    if c == '{' || c == '}' then
      i := i + 1
      continue
    if c == '~' then
      out := out.push ' '
      i := i + 1
      continue
    if c == '-' && i + 1 < cs.size && cs[i+1]! == '-' then
      if i + 2 < cs.size && cs[i+2]! == '-' then
        out := out.push '—'
        i := i + 3
      else
        out := out.push '–'
        i := i + 2
      continue
    out := out.push c
    i := i + 1
  -- Collapse runs of whitespace.
  let words := out.splitToList Char.isWhitespace |>.filter (!·.isEmpty)
  return " ".intercalate words

/-- A person, from `Last, First`, `Last, Jr, First` or `First Last`. -/
private def person (raw : String) : Person :=
  let parts := (raw.splitOn ",").map (·.trimAscii.toString)
  match parts with
  | [last, first] => { full := plain s!"{first} {last}", last := plain last }
  | [last, jr, first] => { full := plain s!"{first} {last}, {jr}", last := plain last }
  | _ =>
    -- `First von Last`: the family name is the last word, or the last braced group.
    let text := raw.trimAscii.toString
    let words := text.splitOn " " |>.filter (!·.isEmpty)
    let last := words.getLast?.getD text
    { full := plain text, last := plain last }

private def people (raw : String) : Array Person :=
  splitNames raw |>.filter (· != "others") |>.map person

/-- Whether a list of names ends with `and others`. -/
def hasOthers (raw : String) : Bool :=
  (splitNames raw).back? == some "others"

/-- The fields of an entry, lowercase, with their raw values. -/
private partial def fields (close : Char) (acc : Array (String × String × Nat)) :
    P (Array (String × String × Nat)) := do
  skipSpace
  if (← peek?) == some close then
    advance
    return acc
  let line := lineAt (← get).chars (← get).pos
  let name := (← identifier "a field name").toLower
  if acc.any (·.1 == name) then fail s!"the field '{name}' appears twice"
  expect '=' s!"'=' after the field '{name}'"
  let v ← value
  skipSpace
  match ← peek? with
  | some ',' => advance; fields close (acc.push (name, v, line))
  | some c =>
    if c == close then advance; return acc.push (name, v, line)
    else fail s!"expected ',' or the end of the entry after the field '{name}', found '{c}'"
  | none => fail "the file ends inside an entry"

/-- Skips the body of `@comment` or `@preamble`. -/
private partial def skipBody (close : Char) : P Unit := do
  if close == '}' then discard braced
  else
    let rec go (depth : Nat) : P Unit := do
      let c ← next
      if c == ')' && depth == 0 then return
      else if c == '(' then go (depth + 1)
      else if c == ')' then go (depth - 1)
      else go depth
    go 0

/-- One entry, after its `@`. -/
private partial def entry : P (Option Entry) := do
  let line := lineAt (← get).chars (← get).pos
  let kind := (← identifier "an entry type after '@'").toLower
  skipSpace
  let close ← match ← peek? with
    | some '{' => advance; pure '}'
    | some '(' => advance; pure ')'
    | some c => fail s!"expected '\{' after @{kind}, found '{c}'"
    | none => fail s!"expected '\{' after @{kind}, but the file ends"
  if kind == "comment" || kind == "preamble" then
    skipBody close
    return none
  if kind == "string" then
    fail "@string abbreviations are not supported: write values in full"
  skipSpace
  let rec keyChars (acc : String) : P String := do
    match ← peek? with
    | some c =>
      if c == ',' || c.isWhitespace || c == close then pure acc
      else if c == '{' || c == '}' || c == '"' || c == '#' || c == '%' || c == '\'' ||
          c == '(' || c == ')' || c == '=' then
        fail s!"'{c}' is not allowed in a key"
      else advance; keyChars (acc.push c)
    | none => pure acc
  let key ← keyChars ""
  if key.isEmpty then fail s!"the @{kind} entry has no key"
  expect ',' s!"',' after the key '{key}'"
  let fs ← fields close #[]
  let get? (name : String) := fs.find? (·.1 == name) |>.map (·.2.1)
  let missing {α} (what : String) : P α :=
    throw s!"line {line}: the entry '{key}' has no {what}"
  let authors := (get? "author").map people |>.getD #[]
  let editors := (get? "editor").map people |>.getD #[]
  if authors.isEmpty && editors.isEmpty then missing "author (or editor)"
  let some title := get? "title" | missing "title"
  let some year := get? "year" | missing "year"
  let venue := ["journal", "booktitle", "publisher", "school", "institution", "organization",
    "howpublished"].findSome? get?
  -- Links are taken as written, but for braces: `~` and `--` are part of them.
  let link (v : String) := (String.ofList (v.toList.filter (fun c => c != '{' && c != '}'))).trimAscii.toString
  let url := (get? "url").map link |>.orElse fun _ => (get? "doi").map fun d =>
    let d := link d
    if d.startsWith "http" then d else s!"https://doi.org/{d}"
  -- Keep the `and others` of a list of names as a last person, shown as "et al.".
  let others (field : String) (ps : Array Person) :=
    if (get? field).any hasOthers then ps.push { full := "others", last := "others" } else ps
  return some {
    key, kind, line
    authors := others "author" authors
    editors := others "editor" editors
    title := plain title, year := plain year
    venue := venue.map plain, url
  }

private partial def entries (acc : Array Entry) : P (Array Entry) := do
  skipSpace
  match ← peek? with
  | none => return acc
  | some '%' =>
    let rec line : P Unit := do
      match ← peek? with
      | some '\n' | none => pure ()
      | some _ => advance; line
    line
    entries acc
  | some '@' =>
    advance
    match ← entry with
    | some e =>
      if let some other := acc.find? (·.key == e.key) then
        throw s!"line {e.line}: the key '{e.key}' is used twice (also on line {other.line})"
      entries (acc.push e)
    | none => entries acc
  | some c =>
    fail s!"unexpected '{c}' outside every entry: entries start with '@', and comments with '%'"

/-- Reads the entries of a BibTeX file, or says what is wrong with it. -/
def parse (text : String) : Except String (Array Entry) :=
  (entries #[]).run' { chars := text.toList.toArray }

/-- The names a citation shows: "A", "A and B", or "A et al.". -/
def Entry.names (e : Entry) : String :=
  let ps := if e.authors.isEmpty then e.editors else e.authors
  let others := ps.any (·.full == "others")
  let ps := ps.filter (·.full != "others")
  match ps.toList with
  | [] => e.key
  | [a] => if others then s!"{a.last} et al." else a.last
  | [a, b] => if others then s!"{a.last} et al." else s!"{a.last} and {b.last}"
  | a :: _ => s!"{a.last} et al."

/-- How a citation shows an entry: its names and year. -/
def Entry.short (e : Entry) : String := s!"{e.names} {e.year}"

/-- A list of people as a reference shows it: "A, B and C". -/
def showPeople (ps : Array Person) : String :=
  let others := ps.any (·.full == "others")
  let names := ps.filter (·.full != "others") |>.map (·.full)
  let listed := match names.toList with
    | [] => ""
    | [a] => a
    | as => ", ".intercalate as.dropLast ++ " and " ++ as.getLast!
  if others then listed ++ " et al." else listed

/-- Ends `s` with a full stop, unless it already ends a sentence. -/
def sentence (s : String) : String :=
  if s.endsWith "." || s.endsWith "?" || s.endsWith "!" then s else s ++ "."

/-- Who an entry is by, as a reference shows it. -/
def Entry.people (e : Entry) : String :=
  if e.authors.isEmpty then s!"{showPeople e.editors} (ed.)" else showPeople e.authors

/-- The full reference as one line of text, for tooltips. -/
def Entry.reference (e : Entry) : String :=
  let venue := e.venue.map (" " ++ sentence ·) |>.getD ""
  s!"{e.people} ({e.year}). {sentence e.title}{venue}"

/-- The order of a bibliography: by first family name, then year, then key. -/
def Entry.sortKey (e : Entry) : String :=
  let ps := if e.authors.isEmpty then e.editors else e.authors
  s!"{(ps[0]?.map (·.last.toLower)).getD e.key.toLower}\u0000{e.year}\u0000{e.key}"

/-- The key and the locator of a `cited` mark: `"Key"` or `"Key, locator"`. -/
def splitCitation (mark : String) : String × Option String :=
  match mark.splitOn "," with
  | [] => ("", none)
  | key :: rest =>
    let loc := ",".intercalate rest |>.trimAscii.toString
    (key.trimAscii.toString, if loc.isEmpty then none else some loc)

end Stemma.BibTeX
