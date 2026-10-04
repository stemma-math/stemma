import Stemma

/-!
# `stemma-extract`

Writes, as JSON, the report of a compiled Stemma library to a file:

```
lake exe stemma-extract <Library> <output.json>
```

The report goes to a file rather than to the standard output, which Lake may
share with messages of its own.
-/

open Lean

def main (args : List String) : IO UInt32 := do
  let [root, output] := args
    | IO.eprintln "usage: stemma-extract <Library> <output.json>"
      return 2
  try
    let root := root.toName
    initSearchPath (← findSysroot)
    unsafe enableInitializersExecution
    let env ← importModules (loadExts := true) #[{ module := root }] {}
    let report ← (Stemma.extract root).toIO { fileName := "", fileMap := default } { env }
    IO.FS.writeFile output (toJson report.1).compress
    return 0
  catch e =>
    IO.eprintln e
    return 1
