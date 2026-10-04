import Stemma

/-!
# `stemma-extract`

Prints, as JSON, the report of a compiled Stemma library:

```
lake exe stemma-extract <Library>
```
-/

open Lean

def main (args : List String) : IO UInt32 := do
  let [root] := args
    | IO.eprintln "usage: stemma-extract <Library>"
      return 2
  try
    let root := root.toName
    initSearchPath (← findSysroot)
    unsafe enableInitializersExecution
    let env ← importModules (loadExts := true) #[{ module := root }] {}
    let report ← (Stemma.extract root).toIO { fileName := "", fileMap := default } { env }
    IO.println (toJson report.1).compress
    return 0
  catch e =>
    IO.eprintln e
    return 1
