# Using lean-audit in CI

The repository ships a composite action. It builds the tool from the pinned ref,
runs the checks you ask for, writes a Markdown report per check, summarises them on
the job page, and fails the job when a check reports a mismatch.

**It never modifies your repository.** It reads sources, manifest files and receipts;
it does not touch Lean sources, and it does not replace `Audit.lean`,
`AxiomCheck.lean` or `scripts/audit_axioms.py`. Those run inside your build; this runs
from outside and records a diffable result.

## Minimal use on tagged releases

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

That runs `inspect`, `manifest` and `scan`. On a repository of a few thousand Lean
files it takes seconds, because none of those three need Lean or a build.

## What the three cheap checks do

| check | what it re-computes | typical cost |
|---|---|---|
| `inspect` | toolchain and Mathlib pins, lakefile targets, `lake-manifest.json`, module list, import graph, the repository's own audit implementation and its head theorems, receipts | seconds |
| `manifest` | every SHA-256 declared in `SOURCE_MANIFEST.json`, `AXIOM_AUDIT.json`, `MANUSCRIPT_BINDING.json`, `SHA256SUMS` and `*.sha256`, plus declared byte lengths | seconds |
| `scan` | forbidden tokens (`sorry`, `admit`, `axiom`, `unsafe`, `implemented_by`, `extern`) in comment- and string-stripped sources, scoped to the module roots your audit file declares with `getRoot` | seconds |
| `axioms` | the head theorems' axiom dependencies, via Lean's own `Lean.collectAxioms`, plus direct `sorryAx` users from `env.constants` | minutes; needs elan and a built repository |

`scan` reports every token it finds, but the **exit code reflects only tokens inside
the scope your audit file declares**. That matters: each of the TEAM42 repositories
documents its `Challenge.lean` `sorry` as deliberate, and a gate that fails on a
documented deliberate `sorry` is useless. Without a declared scope, every local
module stays in scope, because narrowing on our own judgement could hide a real
`sorry`.

## Adding the axiom check

`axioms` needs Lean and a built repository, so it is opt-in and slower:

```yaml
      - uses: ax1s-x1zz/lean-audit@v0.1.0
        with:
          checks: inspect,manifest,scan,axioms
          axioms-args: --build --mode print
```

`--build` runs `lake build` in the target repository first (it needs `elan` on the
runner). `--mode print` reproduces the `#print axioms` code path that
`scripts/audit_axioms.py` uses; the default `collect` mode calls
`Lean.collectAxioms` directly and can also report an unknown head as missing.

To install elan on the runner first:

```yaml
      - run: |
          curl -sSfL https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -o elan-init.sh
          sh elan-init.sh -y --default-toolchain none
          echo "$HOME/.elan/bin" >>"$GITHUB_PATH"
```

## Inputs

| input | default | meaning |
|---|---|---|
| `path` | `.` | repository root to audit |
| `checks` | `inspect,manifest,scan` | comma-separated subset of `inspect`, `manifest`, `scan`, `axioms` |
| `verify-hashes` | `true` | set `false` to report manifest entry counts without recomputing hashes |
| `axioms-args` | *(empty)* | extra arguments for `axioms`, e.g. `--build --mode print` |
| `fail-on-mismatch` | `true` | set `false` to report a mismatch without failing the job |
| `report-dir` | `lean-audit-report` | where the Markdown reports are written |
| `artifact-name` | `lean-audit-report` | name of the uploaded artifact. **Using the action more than once in one workflow run needs a distinct name per use**; the upload overwrites the previous artifact otherwise |

## Outputs

| output | meaning |
|---|---|
| `status` | `ok`, `mismatch` or `error` — the worst across the checks |
| `report-dir` | directory holding `<check>.md` |

## Exit codes

| code | meaning |
|---|---|
| `0` | every check passed |
| `2` | forbidden tokens found in scope |
| `3` | a mismatch was reported (manifest, scan or axioms) |
| `4` | a check could not be run |

A mismatch fails the job only when `fail-on-mismatch` is `true`. **A check that could
not be run always fails the job**, because that is not a pass.

## What it will not tell you

- It does not establish mathematical correctness. A passing run means the published
  verification facts still hold; it says nothing about whether the theorems are true,
  or whether the statements formalise what the paper claims.
- It does not replace a build-time audit or a
  [Comparator](https://github.com/leanprover/comparator) replay. It can tell you
  whether the cheap preconditions still hold before you spend hours on a replay.
- A `not_run` or `parsed_only` result is never reported as `ok`.

## Notes for consumers

- **Using the action twice in one workflow run** (a matrix over repositories, say)
  needs a distinct `artifact-name` per use. The upload sets `overwrite: true` so a
  repeat run does not fail, but two uses in the same run would otherwise leave you
  with only one set of reports. This is exercised by this repository's own CI.
- The action builds the tool from source with `--locked` and caches the build. If you
  prefer to pin third-party actions by commit SHA, add your own checkout of this
  repository and build it, or pin `ax1s-x1zz/lean-audit@<sha>`.
- GitHub-hosted runners ship Rust, so no toolchain step is needed. On a self-hosted
  runner, add a Rust toolchain before the action.
- The reports contain no absolute paths except the audited root, so they are safe to
  attach to a build artifact.
