import Stemma

/-!
# Fingerprints

Properties of the fingerprints: what changes them and what does not.
-/

open Lean Meta Stemma

/-- info: true -/
#guard_msgs in
#eval sha256 "abc" == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"

/-- info: true -/
#guard_msgs in
#eval sha256 "" == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

/-- info: "one two three" -/
#guard_msgs in
#eval normalizeProse "  one\n two\t\tthree  "

def localHash (n : Name) : MetaM String := do
  let some ci := (← getEnv).find? n | throwError "unknown {n}"
  return (← constHash ci).hash

def sameHash (a b : Name) : MetaM Bool := return (← localHash a) == (← localHash b)

-- Proofs inside data do not count.
def withRfl : { n : Nat // n = 1 } := ⟨1, rfl⟩
def withDecide : { n : Nat // n = 1 } := ⟨1, by decide⟩

/-- info: true -/
#guard_msgs in
#eval sameHash ``withRfl ``withDecide

-- Neither do the proofs of theorems.
theorem byRfl : 1 + 1 = 2 := rfl
theorem byDecide : 1 + 1 = 2 := by decide

/-- info: true -/
#guard_msgs in
#eval sameHash ``byRfl ``byDecide

-- Nor the names of bound variables.
def idX : Nat → Nat := fun x => x
def idY : Nat → Nat := fun y => y

/-- info: true -/
#guard_msgs in
#eval sameHash ``idX ``idY

-- Data does.
def two : Nat := 2
def three : Nat := 3

/-- info: false -/
#guard_msgs in
#eval sameHash ``two ``three

-- And so do statements.
theorem small : 2 + 2 = 4 := rfl

/-- info: false -/
#guard_msgs in
#eval sameHash ``byRfl ``small
