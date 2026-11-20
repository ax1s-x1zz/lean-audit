# CHANGELOG

All notable changes to `lean-audit`. Milestone names (M0–M6) refer to
`docs/requirements.md` and the plan in `NEXT.md`.

## [Unreleased]

### Added — M3: axiom re-computation (`axioms`)

- `lean-audit axioms <repo>` generates a Lean file that calls
  `Lean.collectAxioms` for every head theorem and traverses `env.constants` for
  declarations using `sorryAx`, runs it with `lake env lean`, and compares the
  result against the repository's own published claims (`receipts/**/build_summary.txt`
  and `AXIOM_AUDIT.json`).
- `--heads`, `--lake`, `--timeout`, `--build`, `--build-timeout`,
  `--keep-generated`.
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

- 81 tests: 62 unit, 9 stub-based `axioms` integration tests, 3 real-Lean
  integration tests, 7 TEAM42 repository integration tests.
- `tests/fixtures/leanfix` is a project shaped like a TEAM42 repository, used by
  the real-Lean tests.

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
