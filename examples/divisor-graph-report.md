# lean-audit report — RuleHRelease

> **This report verifies reproducibility of published claims. It does not establish mathematical correctness.**

schema `lean-audit/v1` · tool `0.1.0` · generated `2026-11-23T04:50:39Z`

## Source

- path: `/home/x1zz/team42-repos/divisor-graph-longest-path-lean`
- commit: `40c2b7bfd935`

## Toolchain

| field | value | status |
|---|---|---|
| lean-toolchain | `leanprover/lean4:v4.33.1` | ok |
| lean version | `4.33.1` | |
| mathlib (lakefile rev) | `v4.33.1` | ok |
| mathlib rev | `0df444a360eaa60ab8c11dca51a86af692955474` | |

## Lakefile

- kind: `toml` · status: `ok`
- defaultTargets: `RuleHRelease`, `Challenge`, `Solution`, `Audit`, `ExactStatements`, `RequiredAxioms`, `ruleH`, `PublicStatements`

| require | git | rev |
|---|---|---|
| `mathlib` | `https://github.com/leanprover-community/mathlib4.git` | `v4.33.1` |

## Modules

- count: **35** · internal import edges: **90** · max import depth: 16
- external imports: 2 distinct (`Lean`, `Mathlib`)
- roots: `Audit`, `Challenge`, `Check`, `Main`, `PublicStatements`, `RequiredAxioms`, `RuleHRelease`

| directory | .lean files |
|---|---|
| `RuleHRelease` | 26 |
| `(root)` | 9 |

## Audit implementation

- kind: `lean_audit`
- files: `Audit.lean`
- command: `#repo_audit`
- heads: **1** (source: `parsed`)
  - `RuleHRelease.C_offer`
- expected direct `sorry` users: 0
- allowed axioms: `propext`, `Classical.choice`, `Quot.sound`

## Manifests

_none found_

## Receipts

_no `receipts/` directory_

## Forbidden-token scan

- files scanned: 35 · flags: **0** (in scope: **0**)

_tokens checked: sorry, admit, axiom, unsafe, implemented_by, extern; comments and string literals are stripped first; the gate covers module roots RuleHRelease (audit_file)_

---

_This report verifies reproducibility of published claims. It does not establish mathematical correctness._
