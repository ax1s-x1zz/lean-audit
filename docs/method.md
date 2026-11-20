# Method — what `lean-audit` verifies, and what it does not

This document is the contract. If the tool ever contradicts it, the tool is wrong.

---

## 1. The claims it re-computes

A TEAM42 Lean repository publishes, in its `README.md`, `receipts/` and manifest
files, a set of verification facts. `lean-audit` re-computes the ones that can be
checked cheaply, and records agreement or disagreement.

| # | Claim | How it is checked | Milestone |
|---|---|---|---|
| 1 | the Lean toolchain pin | read `lean-toolchain`; compare with the version reported by Lean | **M1 / M3, implemented** |
| 2 | the Mathlib pin | `lakefile.toml` `require` + `lake-manifest.json` rev | **M1, implemented** |
| 3 | the set of modules | enumerate `**/*.lean`, build the import graph | **M1, implemented** |
| 4 | the head theorems depend only on `propext`, `Classical.choice`, `Quot.sound` | generate a Lean file, call `Lean.collectAxioms` for each head, run `lake env lean` | **M3, implemented** |
| 5 | no declaration of the library uses `sorry` directly | `env.constants.map₁` traversal + `getUsedConstantsAsSet.contains ``sorryAx` | **M3, implemented** |
| 6 | `receipts/` numbers (modules built, errors, `sorry` warnings) | re-run and compare with the parsed claims | M6 |
| 7 | `SOURCE_MANIFEST.json` / `MANIFEST.sha256` / `SHA256SUMS` hashes | recompute SHA-256 over the checkout | **M2, implemented** |
| 8 | `data/AXIOM_AUDIT.json` source-hash and toolchain binding | recompute and compare | **M2, implemented** |

M1, M2 and M3 are implemented; M6 is not. Until a check runs, its status is
`parsed_only` or `not_run` — never `ok`. `--no-verify` demonstrates this for M2:
the entry counts are still reported, but `checked` stays `null` and the status
stays `parsed_only`.

## 2. How manifest verification resolves paths

Manifests are discovered by name anywhere in the tree (`SOURCE_MANIFEST.json`,
`AXIOM_AUDIT.json`, `MANUSCRIPT_BINDING.json`, `SHA256SUMS`, `*.sha256`), and four
shapes are understood:

| shape | example |
|---|---|
| `{"files": [{path, bytes, sha256}]}` | `site-percolation-square-lattice-lean/SOURCE_MANIFEST.json` |
| `{"source_files_sha256": {path: sha256}}` | `rubik-stm-entropy-lean/SOURCE_MANIFEST.json` |
| `sha256sum` lines | `data/small-native/MANIFEST.sha256` |
| `{"Lean_source_hashes": {...}, "general_declarations_exact_axioms": {...}}` | `data/AXIOM_AUDIT.json` |

Entry paths follow two conventions, and both appear in the same repository:
`sha256sum` files are normally relative to their own directory
(`data/small-native/MANIFEST.sha256`), but `checker/sharded/PATCH.sha256` and the
JSON manifests use repository-root-relative paths. A file exists in only one
place, so each entry is resolved against the manifest's directory and then the
repository root; which base was used is recorded in the report's `note`. An entry
that resolves nowhere is reported as **missing**, never silently skipped.

`bytes` is compared as well as the hash when the manifest declares it.

## 3. How axiom checking is done — and why it is not string search

`lean-audit` generates a small Lean file and runs it with `lake env lean`:

```lean
import Lean
<the repository's modules>
open Lean Elab Command

elab "#lean_audit" : command => do
  let heads : List Name := [``A, ``B]
  for h in heads do
    let axs := (← liftCoreM <| Lean.collectAxioms h).toList
    logInfo m!"LEANAUDIT_AXIOMS {h} {axs}"
#lean_audit
```

`Lean.collectAxioms` is the same kernel-level API the repositories' own
`Audit.lean` and `AxiomCheck.lean` use. It reports the axioms a declaration
actually depends on, after elaboration.

A `#print axioms` output parser is also provided as a compatibility mode
(`--mode print`), because that is what `scripts/audit_axioms.py` relies on. In
that mode the generated file is exactly `import <modules>` followed by one
`#print axioms <name>` line per head, and the tool parses Lean's messages. The two
modes are independent Lean code paths, and `tests/lean.rs` asserts that they agree
on every head of the fixture project.

Print mode cannot report an unknown head as `missing`: `#print axioms X` fails
elaboration when `X` does not resolve, so the run is reported as `error` instead.
It also has no `env.constants` traversal, and says so in the report rather than
leaving the `sorry` check looking passed. `collect` is the default for both
reasons.

### 3.1 Three decisions that keep the run honest

**Raw `Name` literals for heads.** The generated file writes heads as `` `Name ``
(single backtick), not as ``` ``Name ``` (double backtick). A double-backtick
quotation *resolves* the name in the environment, so one stale head name makes the
whole file fail to elaborate and hides the case worth reporting. With a raw
literal, `env.contains` decides, and an unknown head is reported as
`LEANAUDIT_MISSING` rather than aborting the audit.

**The `sorry` scope is the repository's own.** The traversal is limited to the
module roots the repository's audit file compares against, read from patterns like
`m.getRoot == \`SierksmaLean` or `m == \`Solution`. This matters: `Challenge.lean`
deliberately contains `sorry`, and the repositories exclude it on purpose.
Scanning everything would manufacture a discrepancy that is not one. When no such
comparison exists, the scope falls back to the repository's library directories
(the roots that have subdirectories), and the chosen scope and its source are both
recorded in the report as `sorry_scope` / `sorry_scope_source`.

**`unexpected` means undisclosed, not merely non-standard.** An axiom is reported
as unexpected only when it is neither in the allowed set nor disclosed by a
published claim. `site-percolation`'s small native regressions legitimately depend
on disclosed `native_decide` axioms; those are not surprises.

## 4. The forbidden-token scan

For `sorry`, `admit`, `axiom`, `unsafe`, `implemented_by` and `extern` the scan is
**lexical but comment- and string-aware**. The repository's own
`scripts/audit_axioms.py` documents why: a naive search flags the word inside
comments, doc comments and string literals.

`src/lean_src.rs` ports that logic and adds two properties:

1. **Character-offset preservation.** Every removed character is replaced by
   exactly one space or newline, so an offset in the stripped text still points at
   the same position in the original. This is what lets the tool read the name out
   of `elab "#check_axioms"` — the name is inside a string literal, which the scan
   blanks out.
2. **Identifier-aware matching.** `sorryAx`, `unsafeCast` and `allowSorry` are
   identifiers, not the forbidden tokens, and are not reported. Qualified names
   such as `Foo.axiom` are reported, matching the behaviour of a `\b…\b` regex.

Verified on real data: `ang-subset-products-lean/Challenge.lean` contains a
doc-comment copy of a theorem whose body is `:= by sorry`, plus three real `sorry`
tokens in the actual challenge statements. The scan reports exactly the three real
ones.

## 5. What it does NOT verify

- **It does not establish mathematical correctness.** A successful run means the
  published verification facts still hold; it says nothing about whether the
  theorems are true, or whether the statements formalise what the paper claims.
- **It does not replace `Audit.lean`, `AxiomCheck.lean` or
  `scripts/audit_axioms.py`.** Those run inside the build and fail the build. This
  runs outside, across repositories, and records a diffable result.
- **It does not replace the Comparator replay.** Comparator rebuilds `Challenge`
  and `Solution` in a sandbox, exports both environments and replays the solution
  in the kernel. `lean-audit` cannot do that; it can only tell you whether the
  cheap preconditions still hold before you spend five hours on a replay.
- **It does not trust compiled artefacts differently.** It inherits exactly the
  same trusted base as the repository's own checks.

## 6. Status values

| value | meaning |
|---|---|
| `ok` | recomputed value matches the published claim |
| `mismatch` | recomputed value differs — this is a discrepancy, not an accusation |
| `not_run` | the check was not executed; never reported as `ok` |
| `parsed_only` | the file was read but nothing was recomputed |
| `unsupported` | the repository structure is not what the tool expects; it does not guess |
| `absent` | the artefact (audit file, manifest, receipts) does not exist |
| `error` | the tool itself failed |

## 7. Known limitations

| limitation | effect | policy |
|---|---|---|
| character literals (`'a'`) are not stripped | none: a character literal holds one character and cannot contain a forbidden token | documented, unchanged |
| term quotations (`` `(sorry) ``) are not stripped | a token inside a quotation **is** reported | kept: the conservative direction |
| `#print axioms` message format | parsing breaks if Lean changes the format | `collectAxioms` output is the primary path |
| `receipts/**/build_summary.txt` labels differ per repository | e.g. `modules built (lines '[k/n] Built'): 3683` vs `lines '[k/n] Built <module>' in build.log (…): 25` | the matcher keys on the stable parts and always requires a trailing integer |
| Lean versions differ across the family | 4.33.0, 4.33.1 and 4.35.0 all appear | reported, not normalised; drift is a finding, not an error |
| `lakefile.lean` (Lean, not TOML) | targets and requires are not parsed | reported as `unsupported` |
| manifest entry path base | `sha256sum` files may be self-relative or root-relative | resolved against both; the chosen base is recorded in `note` |
| `axioms` needs the modules built | `lake env lean` cannot import unbuilt modules | reported as `error` with the Lean output tail, never as `ok`; `--build` runs `lake build` first |
| Lean versions differ across the family | 4.33.0, 4.33.1 and 4.35.0 all appear | the toolchain in `lean-toolchain` is what elan selects; drift is a finding, not an error |
| `--build` runs `lake build` in the target repository | two concurrent builds in the same repository race on `.lake` and produce spurious `failed to open file ...olean` errors | observed during development: run one build at a time per repository; the errors are the environment's, not the repository's |

## 8. Honesty rules

These are enforced by review, and encoded in `AGENTS.md`:

1. `not_run` is never reported as `ok`.
2. `sorry` is never decided by string search when a kernel-level answer is
   available.
3. Every report carries the disclaimer, including the JSON.
4. The tool never claims to replace an existing check.
5. A discrepancy is reported as a difference between a claim and a recomputation,
   not as a defect in the repository.
