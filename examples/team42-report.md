# lean-audit batch report

> **This report verifies reproducibility of published claims. It does not establish mathematical correctness.**

root `/home/x1zz/team42-repos` · 8 repositories · tool `0.1.0` · generated `2026-11-20T04:51:59Z`

| project | lean | modules | depth | audit | commands | heads | flags (in/all) | hashes | claims |
|---|---|---|---|---|---|---|---|---|---|
| `AngCounterexample` | 4.33.1 | 26 | 9 | `lean_audit` | `#ang_audit` `#ang_large_audit` | 9 | 0/3 | — | 18 |
| `coth_sums_barnes` | 4.33.0 | 4 | 3 | `none` | — | — | 0/0 | — | 0 |
| `feigenbaum_scalar_duality` | 4.35.0 | 5 | 4 | `lean_audit` | — | 1 | 0/0 | — | 0 |
| `HardDiskB5Lean` | 4.33.1 | 2067 | 56 | `lean_audit` | `#repo_audit` | 2 | 0/1 | — | 2 |
| `HardSphereB5Lean` | 4.33.1 | 1096 | 37 | `lean_audit` | `#repo_audit` | 192 | 0/96 | — | 192 |
| `RubikSTM` | 4.33.1 | 935 | 51 | `lean_axiomcheck` | `#check_axioms` | 30 | 0/0 | 939/939 | 0 |
| `SierksmaLean` | 4.33.1 | 3684 | 33 | `lean_audit` | `#sierksma_audit` | 3 | 0/1 | — | 3 |
| `site_perc_upper` | 4.35.0 | 56 | 10 | `python_script` | — | — | 0/0 | 173/174 ⚠16 | 42 |

## Notes

- `heads` is the number of head theorems found in the repository's own audit file.
- `claims` counts published axiom claims from both sources the tool reads: `AUDIT axioms <name>: [...]` lines in `receipts/**/build_summary.txt`, and the `*_exact_axioms` maps in `AXIOM_AUDIT.json`. Re-computing them is `lean-audit axioms`.
- `hashes` is `matched/checked` across every manifest in the repository; `⚠n` counts entries that are missing from the checkout or whose recomputed hash differs.
- `flags` is `in scope / all`. The gate covers only the module roots the repository's own audit file compares against with `getRoot`, because each repository documents its `Challenge.lean` `sorry` as deliberate. Without a declared scope every local module is in scope.

---

_This report verifies reproducibility of published claims. It does not establish mathematical correctness._
