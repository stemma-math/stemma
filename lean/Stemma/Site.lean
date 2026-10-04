import VersoManual
import Stemma.Genre

/-!
# The library's site

The site is a Verso manual. Its front page is the root module's document, and
its chapters are the library's documents, in the order of the table of
contents. `stemma preview` writes the small program that assembles it, so that
no list of chapters is ever maintained by hand.
-/

namespace Stemma

open Verso Doc
open Verso.Genre Manual

/-- A front page with only a title, for a library whose root module is not a document. -/
def titlePage (title : String) : Part Manual :=
  { title := #[.text title], titleString := title, metadata := none, content := #[],
    subParts := #[] }

/-- The library's book: its front page, with the documents as chapters. -/
def book (front : Part Manual) (documents : Array (Part Manual)) : Part Manual :=
  { front with subParts := front.subParts ++ documents }

end Stemma
