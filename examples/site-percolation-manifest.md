# lean-audit manifest report — site_perc_upper

> **This report verifies reproducibility of published claims. It does not establish mathematical correctness.**

tool `0.1.0` · generated `2026-11-20T04:52:34Z`

| file | kind | entries | checked | matched | missing | mismatched | claims | status |
|---|---|---|---|---|---|---|---|---|
| `SOURCE_MANIFEST.json` | `source_manifest` | 105 | 105 | 105 | 0 | 0 | ? | ok |
| `checker/sharded/PATCH.sha256` | `sha256sum` | 3 | 3 | 2 | 0 | 1 | ? | mismatch |
| `data/AXIOM_AUDIT.json` | `axiom_audit` | 54 | 54 | 54 | 0 | 0 | 42 | ok |
| `data/FLOW_MANIFEST.sha256` | `sha256sum` | 4 | 0 | 0 | 4 | 0 | ? | mismatch |
| `data/MANIFEST.sha256` | `sha256sum` | 11 | 0 | 0 | 11 | 0 | ? | mismatch |
| `data/small-native/MANIFEST.sha256` | `sha256sum` | 12 | 12 | 12 | 0 | 0 | ? | ok |

**Recomputed hash differs** (`checker/sharded/PATCH.sha256`, 1 entries):

- `checker/sharded/shards.py`: expected `ebffeff98290…`, got `29a58b724bc4…`

_checker/sharded/PATCH.sha256 — 3 entries resolved relative to the repository root_

_data/AXIOM_AUDIT.json — also binds recorded axiom lists; re-computation is M3; 54 entries resolved relative to the repository root_

**Not present in this checkout** (`data/FLOW_MANIFEST.sha256`, 4 entries):

- `s1_nodes.bin`
- `s1_edges.bin`
- `s2_nodes.bin`
- `s2_edges.bin`

**Not present in this checkout** (`data/MANIFEST.sha256`, 11 entries):

- `enum.in`
- `group.perm`
- `laws.bin`
- `masses_s1.bin`
- `masses_s2.bin`
- `params.json`
- `region.json`
- `s1_edges.bin`
- `s1_nodes.bin`
- `s2_edges.bin`
- `s2_nodes.bin`

---

_This report verifies reproducibility of published claims. It does not establish mathematical correctness._
