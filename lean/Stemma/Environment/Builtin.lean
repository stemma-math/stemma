import Stemma.Environment.Directive

/-!
# The environments Stemma ships

Groups register their own with `register_environment`.
-/

register_environment definition : definition "Definition"
register_environment construction : definition "Construction"

register_environment «theorem» : statement "Theorem"
register_environment lemma : statement "Lemma"
register_environment proposition : statement "Proposition"
register_environment corollary : statement "Corollary"
register_environment conjecture : statement "Conjecture"

register_environment proof : proof "Proof"

register_environment remark : remark "Remark"
register_environment «example» : remark "Example"
register_environment note : remark "Note"
