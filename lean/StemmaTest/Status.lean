import StemmaTest.Constructions

/--
info: Definition even: central, proved [IsEven]
Theorem even-add: central, proved [IsEven.add]
Proof -: proof of even-add
Lemma even-mul: not central, pending [IsEven.mul]
Conjecture even-open: not central, not formalized
Example -: remark
Construction evens: central, pending [Evens, Evens.add]
Proof -: proof of even-add
Theorem big-even: central, cited [big_even]
Hypothesis all-even: not central, not formalized
-/
#guard_msgs in
#stemma_status
