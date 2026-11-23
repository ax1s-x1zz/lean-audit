import Bad
import Lean

/-- Mirrors the shape of the real repositories' audit files: the `sorry` traversal
    is scoped to a module root with `getRoot`. Here the scope is `Bad`, which is
    exactly where the deliberate `sorry` sits, so the gate must report it. -/
open Lean Elab Command

elab "#in_scope_audit" : command => do
  let env ← getEnv
  let mods := env.header.moduleNames
  let direct := env.constants.map₁.fold (init := (#[] : Array Name)) fun acc n ci =>
    match env.getModuleIdxFor? n with
    | some idx =>
      if mods[idx.toNat]!.getRoot == `Bad && ci.getUsedConstantsAsSet.contains ``sorryAx then
        acc.push n
      else acc
    | none => acc
  logInfo m!"placeholders {direct.toList}"

#in_scope_audit
