/-!
# SHA-256

The digest Stemma's fingerprints use.
-/

namespace Stemma

private def roundConstants : Array UInt32 := #[
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
]

private def initialState : Array UInt32 := #[
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
  0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19
]

private def rotateRight (value distance : UInt32) : UInt32 :=
  (value >>> distance) ||| (value <<< (32 - distance))

private def padded (bytes : ByteArray) : ByteArray := Id.run do
  let bitLength := bytes.size.toUInt64 * 8
  let mut result := bytes.push 0x80
  while result.size % 64 != 56 do
    result := result.push 0
  for shift in [56, 48, 40, 32, 24, 16, 8, 0] do
    result := result.push (((bitLength >>> shift) &&& 0xff).toUInt8)
  return result

private def wordAt (bytes : ByteArray) (offset : Nat) : UInt32 :=
  (bytes[offset]!.toUInt32 <<< 24) |||
  (bytes[offset + 1]!.toUInt32 <<< 16) |||
  (bytes[offset + 2]!.toUInt32 <<< 8) |||
  bytes[offset + 3]!.toUInt32

private def compress (state : Array UInt32) (bytes : ByteArray) (offset : Nat) : Array UInt32 := Id.run do
  let mut words : Array UInt32 := Array.replicate 64 0
  for index in [:16] do
    words := words.set! index (wordAt bytes (offset + index * 4))
  for index in [16:64] do
    let previous15 := words[index - 15]!
    let previous2 := words[index - 2]!
    let sigma0 := rotateRight previous15 7 ^^^ rotateRight previous15 18 ^^^ (previous15 >>> 3)
    let sigma1 := rotateRight previous2 17 ^^^ rotateRight previous2 19 ^^^ (previous2 >>> 10)
    words := words.set! index (words[index - 16]! + sigma0 + words[index - 7]! + sigma1)
  let mut a := state[0]!
  let mut b := state[1]!
  let mut c := state[2]!
  let mut d := state[3]!
  let mut e := state[4]!
  let mut f := state[5]!
  let mut g := state[6]!
  let mut h := state[7]!
  for index in [:64] do
    let sum1 := rotateRight e 6 ^^^ rotateRight e 11 ^^^ rotateRight e 25
    let choice := (e &&& f) ^^^ ((~~~e) &&& g)
    let temporary1 := h + sum1 + choice + roundConstants[index]! + words[index]!
    let sum0 := rotateRight a 2 ^^^ rotateRight a 13 ^^^ rotateRight a 22
    let majority := (a &&& b) ^^^ (a &&& c) ^^^ (b &&& c)
    let temporary2 := sum0 + majority
    h := g
    g := f
    f := e
    e := d + temporary1
    d := c
    c := b
    b := a
    a := temporary1 + temporary2
  return #[
    state[0]! + a, state[1]! + b, state[2]! + c, state[3]! + d,
    state[4]! + e, state[5]! + f, state[6]! + g, state[7]! + h
  ]

private def hexDigit (digit : Nat) : Char :=
  if digit < 10 then
    Char.ofNat ('0'.toNat + digit)
  else
    Char.ofNat ('a'.toNat + digit - 10)

private def hex32 (value : UInt32) : String := Id.run do
  let mut result := ""
  for shift in List.reverse (List.range 8) do
    let digit := ((value >>> (shift * 4).toUInt32) &&& 0xf).toNat
    result := result.push (hexDigit digit)
  return result

/-- The lowercase SHA-256 digest of `value`. -/
def sha256 (value : String) : String := Id.run do
  let bytes := padded value.toUTF8
  let mut state := initialState
  for offset in List.range (bytes.size / 64) do
    state := compress state bytes (offset * 64)
  return String.join (state.toList.map hex32)

end Stemma
