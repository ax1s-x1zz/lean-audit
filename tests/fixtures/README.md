# Test fixtures

Three small Lean projects, each shaped like a TEAM42 research repository. They are
deliberately tiny so the suite runs in seconds and needs no Mathlib.

| fixture | purpose | `scan` exit |
|---|---|---|
| `out-of-scope-sorry` | the only `sorry` is in `Challenge.lean`, and `Audit.lean` scopes the traversal to `Lib` with `getRoot`. Mirrors the real repositories, where the `Challenge.lean` `sorry` is documented as deliberate. | `0` |
| `in-scope-sorry` | the `sorry` is in `Bad.lean` and `Audit.lean` scopes the traversal to `Bad`, so the gate must report it. | `2` |
| `leanfix` | used by the tests that need real Lean. `LeanFix/Stub.lean` has a `sorry` **inside** the declared scope on purpose, so the repository's own audit fails and the tool reports a mismatch against the published expectation of zero. | `2` |

`leanfix` failing `scan` is not an oversight: it exists to exercise the mismatch
path, and `Audit.lean` deliberately declares an expectation of no `sorry` users
while `LeanFix/Stub.lean` has one. `out-of-scope-sorry` is the fixture for the
passing path.

Each fixture's `Audit.lean` uses the same `getRoot` comparison the real repositories
use, because that is what the tool reads to scope the `sorry` traversal and the token
gate.
