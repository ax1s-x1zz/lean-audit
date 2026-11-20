# lean-audit report — RubikSTM

> **This report verifies reproducibility of published claims. It does not establish mathematical correctness.**

schema `lean-audit/v1` · tool `0.1.0` · generated `2026-11-20T02:11:15Z`

## Source

- path: `/home/x1zz/team42-repos/rubik-stm-entropy-lean`
- commit: `8d05092fcca4`

## Toolchain

| field | value | status |
|---|---|---|
| lean-toolchain | `leanprover/lean4:v4.33.1` | ok |
| lean version | `4.33.1` | |
| mathlib (lakefile rev) | `0df444a360eaa60ab8c11dca51a86af692955474` | ok |
| mathlib rev | `0df444a360eaa60ab8c11dca51a86af692955474` | |

## Lakefile

- kind: `toml` · status: `ok`
- defaultTargets: `RubikSTM`, `Main`, `AxiomCheck`

| require | git | rev |
|---|---|---|
| `mathlib` | `https://github.com/leanprover-community/mathlib4` | `0df444a360eaa60ab8c11dca51a86af692955474` |

## Modules

- count: **935** · internal import edges: **2286** · max import depth: 51
- external imports: 11 distinct (`Lean`, `Mathlib`, `Mathlib.Analysis.SpecialFunctions.Log.Basic`, `Mathlib.Data.Fintype.BigOperators`, `Mathlib.Data.List.OfFn`, `Mathlib.GroupTheory.FreeGroup.Reduce`, `Mathlib.GroupTheory.Perm.Cycle.Concrete`, `Mathlib.GroupTheory.SpecificGroups.Alternating`, `Mathlib.GroupTheory.SpecificGroups.Alternating.Simple`, `Mathlib.GroupTheory.SpecificGroups.KleinFour`, `Mathlib.Tactic`)
- roots: `AxiomCheck`, `Check`, `RubikSTM`

| directory | .lean files |
|---|---|
| `RubikSTM` | 931 |
| `(root)` | 4 |

## Audit implementation

- kind: `lean_axiomcheck`
- files: `/home/x1zz/team42-repos/rubik-stm-entropy-lean/AxiomCheck.lean`
- command: `#check_axioms`
- heads: **30** (source: `parsed`)
  - `RubikSTM.rubik_stm_diameter_entropy`
  - `rubik_stm_diameter_entropy`
  - `RubikSTM.rubik_obtm_diameter_entropy`
  - `rubik_obtm_diameter_entropy`
  - `RubikSTM.rubik_stm_solved_radius_entropy`
  - `rubik_stm_solved_radius_entropy`
  - `RubikSTM.rubik_stm_solved_looking_radius_entropy`
  - `rubik_stm_solved_looking_radius_entropy`
  - `RubikSTM.rubik_obtm_solved_radius_entropy`
  - `rubik_obtm_solved_radius_entropy`
  - `RubikSTM.rubik_obtm_solved_looking_radius_entropy`
  - `rubik_obtm_solved_looking_radius_entropy`
  - `RubikConstruction.EntropySharpDiameter`
  - `RubikOBTM.OBTMSharpDiameter`
  - `RubikSolved.STMSolvedRadiusEntropy`
  - `RubikSolved.STMSolvedLookingRadiusEntropy`
  - `RubikSolved.OBTMSolvedRadiusEntropy`
  - `RubikSolved.OBTMSolvedLookingRadiusEntropy`
  - `RubikConvention.UnitRotationSharpDiameter`
  - `RubikSTM.rubik_obtm_unit_rotation_diameter_entropy`
  - `rubik_obtm_unit_rotation_diameter_entropy`
  - `RubikConvention.FreeRotationSharpDiameter`
  - `RubikSTM.rubik_obtm_free_rotation_diameter_entropy`
  - `rubik_obtm_free_rotation_diameter_entropy`
  - `RubikConvention.RotationQuotientSharpDiameter`
  - `RubikSTM.rubik_obtm_rotation_quotient_diameter_entropy`
  - `rubik_obtm_rotation_quotient_diameter_entropy`
  - `RubikConvention.ShortOnlySharpDiameter`
  - `RubikSTM.rubik_obtm_short_only_diameter_entropy`
  - `rubik_obtm_short_only_diameter_entropy`
- expected direct `sorry` users: 0
- allowed axioms: `propext`, `Classical.choice`, `Quot.sound`

## Manifests

| file | kind | entries | checked | matched | missing | mismatched | claims | status |
|---|---|---|---|---|---|---|---|---|
| `SOURCE_MANIFEST.json` | `source_manifest` | 939 | 939 | 939 | 0 | 0 | ? | ok |

## Receipts

_no `receipts/` directory_

## Forbidden-token scan

- files scanned: 935 · flags: **0**

_tokens checked: sorry, admit, axiom, unsafe, implemented_by, extern; comments and string literals are stripped first_

---

_This report verifies reproducibility of published claims. It does not establish mathematical correctness._
