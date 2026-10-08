import Stemma.BibTeX

/-!
The BibTeX reader reads what the site shows, and fails clearly, naming the
line, on anything malformed rather than skipping it.
-/

open Stemma.BibTeX

def render (text : String) : String :=
  match parse text with
  | .ok es => " | ".intercalate (es.toList.map fun e => s!"{e.key}: {e.short}: {e.reference}")
  | .error e => s!"error: {e}"

/--
info: "BurrisSankappanavar: Burris and Sankappanavar 1981: Stanley Burris and H. P. Sankappanavar (1981). A Course in Universal Algebra. Springer. | Godel31: Gödel 1931: Kurt Gödel (1931). Über formal unentscheidbare Sätze. Monatsh. Math."
-/
#guard_msgs in
#eval render "% A comment.
@book{BurrisSankappanavar,
  author = {Burris, Stanley and H. P. Sankappanavar},
  title = {A Course in {U}niversal {A}lgebra},
  publisher = \"Springer\",
  year = 1981,
}
@comment{Ignored, {even with braces}.}
@article(Godel31, author = {Kurt G{\\\"o}del}, title = {{\\\"U}ber formal unentscheidbare S\\\"atze},
  journal = {Monatsh. Math.}, year = {1931})"

/-- info: "A: One et al. 2001: A. One et al. (2001). T." -/
#guard_msgs in
#eval render "@misc{A, author = {One, A. and others}, title = {T}, year = {2001}}"

/-- info: "error: line 1: the entry 'A' has no year" -/
#guard_msgs in
#eval render "@book{A, author = {X}, title = {T}}"

/-- info: "error: line 1: the entry 'A' has no author (or editor)" -/
#guard_msgs in
#eval render "@book{A, title = {T}, year = 2000}"

/-- info: "error: line 3: the key 'A' is used twice (also on line 1)" -/
#guard_msgs in
#eval render "@book{A, author = {X}, title = {T}, year = 1}\n\n@book{A, author = {Y}, title = {U}, year = 2}"

/-- info: "error: line 1: expected ',' or the end of the entry after the field 'author', found 't'" -/
#guard_msgs in
#eval render "@book{A, author = {X} title = {T}, year = 1}"

/-- info: "error: line 2: the file ends inside an entry" -/
#guard_msgs in
#eval render "@book{A, author = {X},\n title = {T, year = 1}"

/-- info: "error: line 1: unexpected '}' outside every entry: entries start with '@', and comments with '%'" -/
#guard_msgs in
#eval render "@book{A, author = {X}, title = {T}, year = 1}}"

/-- info: "error: line 1: unexpected 'S' outside every entry: entries start with '@', and comments with '%'" -/
#guard_msgs in
#eval render "Stray text"

/-- info: "error: line 1: 'jan' is an @string abbreviation, which is not supported: write the value in braces" -/
#guard_msgs in
#eval render "@book{A, author = {X}, title = {T}, year = 1, month = jan}"

/-- info: "error: line 1: the field 'title' appears twice" -/
#guard_msgs in
#eval render "@book{A, author = {X}, title = {T}, title = {U}, year = 1}"

/-- info: ("Key2001", some "Prop. 3.4") -/
#guard_msgs in
#eval splitCitation "Key2001, Prop. 3.4"

/-- info: ("Key2001", none) -/
#guard_msgs in
#eval splitCitation " Key2001 "
