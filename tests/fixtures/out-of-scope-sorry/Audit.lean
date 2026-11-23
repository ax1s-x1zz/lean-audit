import Lib.Ok
import Lean

/-- The `sorry` traversal is scoped to `Lib` with `getRoot`, exactly as the real
    repositories scope theirs, so `Challenge.lean`'s deliberate `sorry` is out of
    scope and the token gate must pass. Used by the CI self-test. -/
open Lean Elab Command

elab "#out_of_scope_audit" : command => do
  let env ← getEnv
  let mods := env.header.moduleNames
  let direct := env.constants.map₁.fold (init := (#[] : Array Name)) fun acc n ci =>
    match env.getModuleIdxFor? n with
    | some idx =>
      if mods[idx.toNat]!.getRoot == `Lib && ci.getUsedConstantsAsSet.contains ``sorryAx then
        acc.push n
      else acc
    | none => acc
  logInfo m!"placeholders {direct.toList}"

#out_of_scope_audit
