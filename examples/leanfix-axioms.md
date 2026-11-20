# lean-audit axioms report — leanfix

> **This report verifies reproducibility of published claims. It does not establish mathematical correctness.**

tool `0.1.0` · generated `2026-11-20T02:27:54Z` · status **mismatch**

| field | value |
|---|---|
| lean executed | true |
| mode | `collect` |
| lake | `lake` |
| heads from | `audit_file` |
| return code | 0 |
| wall time | 7.4s |
| allowed axioms | `propext`, `Classical.choice`, `Quot.sound` |
| imports | `LeanFix`, `Lean` |

## Heads

| head | recomputed axioms | published claim | matches | disclosed |
|---|---|---|---|---|
| `head_true` | _(none)_ | — | — | yes |
| `head_classical` | `propext`, `Classical.choice`, `Quot.sound` | — | — | yes |

## Direct `sorry` users

- recomputed from `env.constants`: **1**
- published expectation: **0**
- ⚠ recomputed list differs from the published expectation
  - `stub_unproved`

_generated file: `/tmp/lean-audit-10733.lean`_

---

_This report verifies reproducibility of published claims. It does not establish mathematical correctness._
