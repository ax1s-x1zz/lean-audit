import LeanFix
import Lean

/-! Build-time audit, mirroring the shape used by the TEAM42 repositories. -/

open Lean Elab Command

elab "#leanfix_audit" : command => do
  let env ← getEnv
  let std : List Name := [``propext, ``Classical.choice, ``Quot.sound]
  let expected : List Name := []
  let heads : List Name := [``head_true, ``head_classical]
  for h in heads do
    let axs := (← liftCoreM <| Lean.collectAxioms h).toList
    logInfo m!"AUDIT axioms {h}: {axs}"
    for a in axs do
      unless std.contains a do
        logError m!"AUDIT FAIL {h}: forbidden axiom {a}"
  let mods := env.header.moduleNames
  let direct := env.constants.map₁.fold (init := (#[] : Array Name)) fun acc n ci =>
    match env.getModuleIdxFor? n with
    | some idx =>
      if mods[idx.toNat]!.getRoot == `LeanFix && ci.getUsedConstantsAsSet.contains ``sorryAx then
        acc.push n
      else acc
    | none => acc
  let found := (direct.qsort Name.lt).toList
  let want := (expected.toArray.qsort Name.lt).toList
  logInfo m!"AUDIT placeholders ({found.length}): {found}"
  unless found == want do
    logError m!"AUDIT FAIL placeholder mismatch: found {found}, expected {want}"

#leanfix_audit
