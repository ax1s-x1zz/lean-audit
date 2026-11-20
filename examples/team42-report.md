# lean-audit batch report

> **This report verifies reproducibility of published claims. It does not establish mathematical correctness.**

root `/home/x1zz/team42-repos` · 5 repositories · tool `0.1.0` · generated `2026-11-20T02:27:43Z`

| project | lean | modules | depth | audit | commands | heads | flags | hashes | claims |
|---|---|---|---|---|---|---|---|---|---|
| `AngCounterexample` | 4.33.1 | 26 | 9 | `lean_audit` | `#ang_audit` `#ang_large_audit` | 9 | 3 | — | 18 |
| `coth_sums_barnes` | 4.33.0 | 4 | 3 | `none` | — | — | 0 | — | 0 |
| `feigenbaum_scalar_duality` | 4.35.0 | 5 | 4 | `lean_audit` | — | 1 | 0 | — | 0 |
| `RubikSTM` | 4.33.1 | 935 | 51 | `lean_axiomcheck` | `#check_axioms` | 30 | 0 | 939/939 | 0 |
| `site_perc_upper` | 4.35.0 | 56 | 10 | `python_script` | — | — | 0 | 173/174 ⚠16 | 0 |

## Notes

- `heads` is the number of head theorems found in the repository's own audit file.
- `claims` is the number of published `AUDIT axioms <name>: [...]` lines parsed from `receipts/`. Re-computing them is M3.
- `hashes` is `matched/checked` across every manifest in the repository; `⚠n` counts entries that are missing from the checkout or whose recomputed hash differs.
- `flags` counts forbidden tokens (`sorry`, `admit`, `axiom`, `unsafe`, `implemented_by`, `extern`) in comment- and string-stripped source.

---

_This report verifies reproducibility of published claims. It does not establish mathematical correctness._
