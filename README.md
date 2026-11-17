# lean-audit

A read-only auditor for Lean 4 research repositories. It re-computes the verification facts that a repository **publishes about itself** — toolchain pins, manifest hashes, the axiom dependencies of its head theorems, and direct `sorry` usage — and reports whether those facts still hold in the checkout you have.

**It does not replace `Audit.lean`, and it does not check mathematics.**

Repositories such as those from TEAM42 already audit themselves *at build time*. This tool audits them *from the outside*, across repositories, cheaply, and records the result in a diffable format.

## Why this exists

TEAM42's Lean repositories implement the same axiom/`sorry` audit **three different ways**:

| repository | implementation |
|---|---|
| `sierksma33`, `ang-…`, `feigenbaum-…`, `hard-disk-b5-…`, `hard-sphere-b5-…` | `Audit.lean` — Lean elaborator command `#sierksma_audit` |
| `rubik-stm-entropy-lean` | `AxiomCheck.lean` — Lean elaborator command `#check_axioms` (30 heads) |
| `site-percolation-…` | `scripts/audit_axioms.py` — Python, generates a temp Lean file, runs `lake env lean` |

All three rely on the same kernel-level contract:

- allowed axioms: exactly `propext`, `Classical.choice`, `Quot.sound`
- no direct `sorryAx` user inside the repository's own library
- forbidden tokens: `sorry`, `admit`, `axiom`, `unsafe`, `implemented_by`, `extern`

Meanwhile, re-checking the published claims requires a clean build — **33 GB of disk and 39 minutes** for `sierksma33`, or a **5-hour** Comparator replay. So, in practice, nobody re-checks.

## Role, relative to what already exists

| | existing | `lean-audit` |
|---|---|---|
| where it runs | inside the repository | **outside** |
| when | at build time (must build successfully first) | **afterwards, any time** |
| scope | one repository | **many repositories** |
| implementation | rewritten per repository | **one binary** |
| comparison | none | **drift diff between runs** |
| record | hand-collected `receipts/` | **schema + hashes** |
| cost | clean build (39 min – 5 h) | **incremental, minutes** |

## How it checks axioms

It calls **Lean's own** `Lean.collectAxioms` and `env.constants` traversal by generating a small Lean file and running it with `lake env lean`. It **never string-searches** for `sorry`, so comment and string-literal false positives cannot occur. `#print axioms` output parsing is available as a compatibility mode.

## What it does not do

- It does **not** decide whether a theorem is mathematically true.
- It does **not** replace `Audit.lean`, `AxiomCheck.lean`, or `scripts/audit_axioms.py`.
- It does **not** replace the [Comparator](https://github.com/leanprover/comparator) replay.
- It does **not** force a clean build.

Every report carries this disclaimer:

> This report verifies reproducibility of published claims. It does **not** establish mathematical correctness.

## Status

Early, but working against real repositories. `inspect`, `scan` and `batch` are
implemented; axiom re-computation (`axioms`) is next. See
`docs/requirements.md` for the reference-implementation analysis,
`docs/method.md` for the exact verification contract, and `NEXT.md` for the
working queue.

## Usage

```sh
# toolchain pins, lakefile, lake-manifest, module list, import graph, audit detection
lean-audit inspect ../team42-repos/rubik-stm-entropy-lean

# JSON output, plus the module import graph
lean-audit inspect ../team42-repos/rubik-stm-entropy-lean --format json --out audit.json
lean-audit inspect ../team42-repos/rubik-stm-entropy-lean --emit-graph graph.json

# forbidden-token scan (comment/string aware); exit 2 when tokens are found
lean-audit scan ../team42-repos/ang-subset-products-lean

# every Lean project under a directory, summarised together
lean-audit batch ../team42-repos
```

Exit code is `0` on success and `2` when forbidden tokens are found, so the tool
can gate CI. Finding no tokens is not a correctness claim.

### Example output

`examples/team42-report.md` is a real batch run over five TEAM42 research
repositories. It reports, among other things, that the family currently spans
**three Lean toolchains** (4.33.0, 4.33.1, 4.35.0) and that one repository
documents its axiom check manually in the README rather than enforcing it at
build time.

## License

Apache-2.0.
