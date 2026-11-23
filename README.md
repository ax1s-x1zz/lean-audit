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

Working against real repositories, and against real Lean. `inspect`, `manifest`,
`scan`, `batch`, `axioms` and `drift` are implemented. `axioms` calls Lean's own
`collectAxioms` and `env.constants` traversal through a generated file run with
`lake env lean`. `drift` compares two report JSON files so a tag watcher can
report only what changed. See `docs/requirements.md` for the reference-implementation
analysis, `docs/method.md` for the exact verification contract, and `NEXT.md` for
the working queue.

## Usage

```sh
# toolchain pins, lakefile, lake-manifest, module list, import graph, audit detection
lean-audit inspect ../team42-repos/rubik-stm-entropy-lean

# JSON output, plus the module import graph
lean-audit inspect ../team42-repos/rubik-stm-entropy-lean --format json --out audit.json
lean-audit inspect ../team42-repos/rubik-stm-entropy-lean --emit-graph graph.json

# re-compute every manifest hash in the repository
lean-audit manifest ../team42-repos/site-percolation-square-lattice-lean

# re-compute the head theorems' axioms with Lean (needs elan; --build first time)
lean-audit axioms ../team42-repos/ang-subset-products-lean --build

# cross-check against the reference implementation's `#print axioms` code path
lean-audit axioms ../team42-repos/ang-subset-products-lean --mode print

# forbidden-token scan (comment/string aware); exit 2 when tokens are found
lean-audit scan ../team42-repos/ang-subset-products-lean

# every Lean project under a directory, summarised together
lean-audit batch ../team42-repos

# compare two reports and show only what changed (exit 3 when anything changed)
lean-audit drift before.json after.json

# watch a group's tags and diff each new tag against the previous observation
scripts/watch-tags.sh ../team42-repos .watch TEAM-42KR
```

Exit codes: `0` success, `2` forbidden tokens found, `3` mismatch (manifest or
axiom) or drift between two reports, `4` the check could not be run. The tool is
therefore usable as a CI gate. Finding nothing is not a correctness claim.

### Example output

`examples/team42-report.md` is a real batch run over the nine Lean repositories
of one research group. `examples/site-percolation-manifest.md` and
`examples/hard-sphere-scan.md` are the manifest and scan reports for two of them,
`examples/divisor-graph-report.md` and `examples/divisor-graph-python-manifest.md`
cover two more added after the first pass, and `examples/ang-axioms.md` is a real
Lean run. `examples/drift-example.md` diffs the batch report across a release.
Together they report, among other things:

- the group spans **three Lean toolchains** (4.33.0, 4.33.1, 4.35.0) and **five
  distinct audit command names**, plus a `run_cmd` variant, a Python script, and
  two repositories with no audit at all;
- `rubik-stm-entropy-lean` verifies **939/939** published source hashes, and
  `site-percolation-square-lattice-lean` verifies **105/105**;
- all **nine head theorems** of `ang-subset-products-lean` recompute to exactly the
  axioms its `receipts/` publish, and `env.constants` reports zero direct `sorry`
  users;
- across **7,873 Lean files** there is no `axiom`, `unsafe`, `implemented_by` or
  `extern` declaration, and every `sorry` token sits in a `Challenge.lean` file
  that the repository documents as deliberate;
- in `site-percolation-square-lattice-lean`, the hash recorded in
  `checker/sharded/PATCH.sha256` for `checker/sharded/shards.py` **did not match
  the shipped file** at v1.1.1, which is unmodified relative to git HEAD. That file
  is not a leftover — `SOURCE_MANIFEST.json` hashes it and
  `scripts/export_source.py` ships it — so it was an exported receipt whose
  recorded hash was stale. The maintainer confirmed this and regenerated it in
  v1.1.2; the check now reports **3/3**
  (`examples/site-percolation-v1.1.2-manifest.md`);
- 15 entries in `data/MANIFEST.sha256` and `data/FLOW_MANIFEST.sha256` are absent
  from the checkout; they are the `inputs/` and `certificates/` data of the
  certificate archive published with the paper (530,966,191 bytes, 9.9 GB
  uncompressed).

`axioms` is exercised end to end against real Lean by
`tests/lean.rs`, which builds `tests/fixtures/leanfix` — a project shaped like a
TEAM42 repository, with a library directory, a `Challenge.lean` whose `sorry` is
intentional, and an audit file that scopes its `sorry` scan with
`getRoot == \`LeanFix`.

## Using it in CI

The repository ships a composite action that builds the tool from the pinned ref,
runs the checks you ask for, writes a Markdown report per check, and fails the job on
a mismatch. It never modifies your repository.

```yaml
name: lean-audit
on:
  push:
    tags: ['v*']
jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ax1s-x1zz/lean-audit@v0.1.0
```

`inspect`, `manifest` and `scan` take seconds and need neither Lean nor a build. The
`axioms` check is opt-in and slower. Inputs, outputs, exit codes and the scoping rule
are documented in [`docs/ci.md`](docs/ci.md).

## Report conventions

Every path inside a report is **relative to `source.path`**, which appears exactly
once and identifies the checkout that was audited. That keeps a published report
portable and free of the machine's directory layout. `axioms` reports the path of
the generated Lean file it used, and the import graph lists module names only.

Exit codes: `0` success, `2` forbidden tokens found in scope, `3` mismatch
(manifest or axiom), `4` the check could not be run. The tool is therefore usable
as a CI gate. Finding nothing is not a correctness claim.

## License

Apache-2.0.
