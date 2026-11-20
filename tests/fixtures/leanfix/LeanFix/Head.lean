import LeanFix.Base

theorem head_true : True := trivial
theorem head_classical (p : Prop) : p ∨ ¬p := Classical.em p
