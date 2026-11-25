# CHANGELOG

All notable changes to `lean-audit`. Milestone names (M0–M6) refer to
`docs/requirements.md` and the plan in `NEXT.md`.

## [Unreleased]

### Added — M6: build + receipts re-computation (`build`)

- `lean-audit build <repo>` runs `lake build`, parses the `[k/n] Built <module>
  (Xs)` progress lines into per-module times, counts `error:` lines and
  `declaration uses 'sorry'` warnings, and compares those against the numbers the
  repository publishes in `receipts/**/build_summary.txt`.
- `--clean` runs `lake clean` first. Without it the module count reflects only
  what lake rebuilt, so the count comparison is reported as **`not_run`**, never
  as a match — the receipt was produced from a clean build.
- `--top N` caps the slowest-modules table; `--timeout` bounds the build.
- Exit code 3 on a claim mismatch, 4 when the build could not run or failed.
- The parser handles `(1.8s)`, `(12s)` and `(171ms)`; the millisecond case was a
  real bug found by running against the fixture.
- `tests/build.rs` drives the real binary against `tests/fixtures/buildclaim`
  (one module + a receipt): a matching claim exits 0, a wrong count exits 3.
  `tests/fixtures/buildclaim/` is the first fixture that ships a `receipts/`.
- `docs/method.md` §7 records that a full build may not finish on a small host,
  so `build`/`axioms` can stay `not_run` for large repositories.

### Added — M5: drift between two reports

- `lean-audit drift <old.json> <new.json>` compares two `inspect` or `batch`
  reports and prints only the fields that changed: toolchain and Mathlib pins, the
  source commit, lakefile targets and requires, module counts and import depth,
  audit command names and head theorems, allowed and expected-sorry lists,
  forbidden-token flags, per-manifest status and counts, and published axiom
  claims from receipts. Batch reports are matched by project name; a project
  present in only one side is reported as such.
- Exit code 3 when anything changed, so a watcher can gate on it. "No drift" means
  the two reports agree, not that the repository is correct.
- `scripts/watch-tags.sh` watches a set of clones for new tags, runs `inspect`
  (which re-computes manifest hashes) on each new tag, diffs against the previous
  observation, and stays silent when nothing changed. `AXIOMS=1` adds the slow
  `axioms --build` check.
- `tests/drift.rs` drives the real binary: identical reports exit 0, one changed
  field exits 3.
- `examples/drift-example.md` is a real diff of the batch report across the
  v1.1.1 → v1.1.2 release.

### Added — M7: composite action for CI

- `action.yml` builds the tool from the pinned ref with `--locked` and a cache keyed
  on its own `Cargo.lock`, runs the requested checks, writes a Markdown report per
  check, summarises them on the job page, and fails the job on a mismatch. It never
  modifies the audited repository.
- `scripts/ci.sh` holds the gate logic, so it is testable without GitHub.
- Inputs: `path`, `checks`, `verify-hashes`, `axioms-args`, `fail-on-mismatch`,
  `report-dir`. Outputs: `status`, `report-dir`.
- A check that **cannot be run** always fails the job, whatever `fail-on-mismatch`
  says, because that is not a pass.
- `docs/ci.md` documents usage, exit codes, the scoping rule, and what the tool will
  not tell you.
- Two new fixtures, `out-of-scope-sorry` (the gate passes) and `in-scope-sorry` (the
  gate fails), with `tests/gate.rs` covering both paths plus unknown checks, a
  missing binary, report writing, and the cannot-run case.

### Added — M3: axiom re-computation (`axioms`)

- `lean-audit axioms <repo>` generates a Lean file that calls
  `Lean.collectAxioms` for every head theorem and traverses `env.constants` for
  declarations using `sorryAx`, runs it with `lake env lean`, and compares the
  result against the repository's own published claims (`receipts/**/build_summary.txt`
  and `AXIOM_AUDIT.json`).
- `--heads`, `--lake`, `--timeout`, `--build`, `--build-timeout`,
  `--keep-generated`.
- `--mode collect` (default) uses a generated `elab` command calling
  `Lean.collectAxioms`; `--mode print` reproduces the `#print axioms` path of the
  reference `scripts/audit_axioms.py`. `tests/lean.rs` asserts that the two code
  paths agree on every head of the fixture project.
- Exit code 3 on mismatch, 4 when the check could not be run.

### Added — M2: manifest verification (`manifest`)

- Discovers `SOURCE_MANIFEST.json` (both the `files[]` and the
  `source_files_sha256{}` shapes), `AXIOM_AUDIT.json`, `MANUSCRIPT_BINDING.json`,
  `SHA256SUMS` and `*.sha256` anywhere in the tree, and recomputes every declared
  SHA-256, plus the declared byte length when present.
- Entry paths are resolved against the manifest's own directory and then the
  repository root; which base was used is recorded. Entries that resolve nowhere
  are reported `missing`, never skipped.
- `inspect` and `batch` verify by default; `--no-verify` reports entry counts only
  and keeps the status `parsed_only`.
- Exit code 3 on mismatch.

### Added — M1: repository inspection (`inspect`, `scan`, `batch`)

- Toolchain and Mathlib pins, lakefile targets and requires, `lake-manifest.json`
  packages, module list, import graph (edges, roots, max depth, cycles), audit
  implementation detection with head-theorem extraction, receipts parsing, and a
  comment/string-aware forbidden-token scan.
- `batch` summarises every Lean project under a directory.
- Exit code 2 when forbidden tokens are found.

### Verified

- 99 tests: 66 unit, 11 stub-based `axioms` integration tests, 7 gate tests, 4
  real-Lean integration tests, 11 TEAM42 repository integration tests.
- `tests/fixtures/leanfix` is a project shaped like a TEAM42 repository, used by
  the real-Lean tests. `tests/fixtures/{out-of-scope-sorry,in-scope-sorry}` cover
  the CI gate's passing and failing paths without needing Lean.

### Findings on real repositories

Recorded in `NEXT.md` and `examples/`. Notable:

- `rubik-stm-entropy-lean`: `SOURCE_MANIFEST.json` 939/939 hashes match.
- `site-percolation-square-lattice-lean`: `SOURCE_MANIFEST.json` 105/105,
  `data/AXIOM_AUDIT.json` 54/54 (42 recorded axiom claims),
  `data/small-native/MANIFEST.sha256` 12/12.
- `site-percolation-square-lattice-lean`: the hash recorded in
  `checker/sharded/PATCH.sha256` for `checker/sharded/shards.py` does not match
  the shipped file, which is unmodified relative to git HEAD.
- `site-percolation-square-lattice-lean`: 15 entries in `data/MANIFEST.sha256`
  and `data/FLOW_MANIFEST.sha256` are absent from the checkout; the README states
  that data ships with the paper.
- The repository family spans three Lean toolchains (4.33.0, 4.33.1, 4.35.0) and
  four different audit implementations.
- `coth-sums-barnes-lean` has no audit implementation; its README documents a
  manual `#print axioms` procedure instead.

### Design decisions worth knowing

- Axioms are decided by Lean's own `collectAxioms`, never by string search. The
  forbidden-token scan is lexical but strips nested comments and string literals
  first, and preserves character offsets so a value inside a string literal (such
  as `elab "#check_axioms"`) can still be read from the original text.
- Head names are emitted as raw `Name` literals: a double-backtick quotation
  resolves the name, so one stale head would fail elaboration and hide the case
  worth reporting.
- The `sorry` traversal is scoped to the roots the repository's own audit file
  compares against, so a deliberate `Challenge.lean` `sorry` is not reported as a
  discrepancy.
- An axiom is `unexpected` only when it is neither allowed nor disclosed by a
  published claim.
- Subprocess timeouts cannot be stalled by a grandchild holding the pipe.
