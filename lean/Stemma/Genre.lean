import VersoManual

/-!
# The `Stemma` genre

Documents in a Stemma library are written in the `Stemma` genre. For now it is
Verso's `Manual` genre under another name; it becomes its own as Stemma needs.
-/

/-- The genre of Stemma documents. -/
abbrev Stemma : Verso.Doc.Genre := Verso.Genre.Manual
