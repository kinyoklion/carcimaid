# Roadmap

Priorities are derived from the compliance baseline — the structural diff of
carcimaid's SVG against the mermaid CLI across the 199-diagram flowchart corpus.
Run it yourself with `cargo run -p compliance -- --corpus corpus/flowchart/mermaid`.

## Baseline snapshot (flowchart corpus)

After text metrics + dagre + subgraphs + rhombus/circle (commit 986d6b1):

- 199 diagrams, 196 compared (3 are unrenderable by the mermaid CLI itself).
- **Exact passes: 6.** Geometry is correct: node coords, edge curves, viewBox,
  clusters, and rhombus/circle shapes all match byte-for-byte where structure
  aligns. Synthetic single-word rhombus/circle diagrams pass.
- **Label wrapping is now the dominant gate**: nearly every corpus diagram using
  a rhombus/circle (or a long rect label) has a multi-word label that mermaid
  *wraps* into multiple `tspan` rows; we emit one row, so those diagrams can't
  reach 0 diffs despite exact geometry. This blocks the next batch of passes.
- Per-case diffs: median ~24 (noisy — judge by passes, below).

**Key finding:** node coordinates, edge `curveBasis` routing, and viewBox now
match mermaid *exactly* wherever the element trees line up (that's the 5 passes).
But the structural diff is recursive and order-sensitive: as soon as a diagram
contains a feature we don't model yet (a subgraph, a multi-line label, an exotic
shape), child counts diverge and **everything downstream is compared against the
wrong mermaid element**. So the `transform`/`d` counts stay high not because the
coordinates are wrong but because they're misaligned. The bottleneck has moved
from geometry to **structural coverage**.

Unsupported structural features, by corpus frequency:

| feature | % of corpus | effect |
|---------|-------------|--------|
| subgraphs (`subgraph … end`) | 31% | unrendered `g.clusters`; large cascades |
| exotic shapes (hexagon, cylinder, trapezoid, …) | 24% | `polygon`/`path` vs our `rect` |
| `<br>` / multi-line labels | 18% | extra `tspan` rows; size + child-count diffs |
| `click`/links | 13% | mermaid wraps the node in `<a>` |

## On the diff-count metric

Raw diff-count is **noisy for partially-correct complex diagrams**: the
comparator matches children positionally, so once a subtree we don't yet model
appears, everything after it misaligns and the count balloons (or, conversely,
an *unrendered* feature makes the tree bail early and undercounts). Adding a
feature can therefore *raise* the count while making the output more correct —
e.g. rendering clusters exposed more comparable-but-imperfect detail on complex
subgraphs. **Exact passes (0 diffs) is the reliable north star**; tag-similarity
is a useful secondary. Judge features by passes + near-passes, not total diffs.

## Next milestones, in impact order

1. **Text measurement** — DONE (f112648). DejaVu Sans via `ttf-parser`.
2. **Dagre layout + curveBasis edges** — DONE (b6f2c39). First exact passes.
3. **Subgraphs / clusters** — DONE (4d1dcb8, 31d829f). Parse `subgraph … end`,
   model as dagre compound nodes, render `g.clusters`. Clean subgraph diagrams
   now pass; no effect on non-subgraph diagrams. Remaining subgraph gaps:
   node/cluster emission ordering, nested-subgraph-by-reference, per-subgraph
   `direction`.
4. **Node shape coverage** — IN PROGRESS (986d6b1). rhombus + circle now exact
   (full passes for single-line labels). Method: per-shape *additive* size
   calibration over our measured text width (rhombus +49, circle 2r=text+14.8)
   plus mermaid's point formula from source. Note: mermaid's internal
   `text.getBBox()` can't be reproduced exactly from our metric, so each shape
   needs empirical calibration rather than a shared padding constant. Remaining:
   polygon family (hexagon/trapezoids/parallelogram/subroutine — additive,
   calibratable) and path shapes (stadium/cylinder — bezier/arc paths, hard).
5. **Rich labels / wrapping** — mermaid wraps labels to a max width, emitting one
   `tspan` row per line; narrow shapes (diamonds) wrap sooner. This is entangled
   with shapes (a rhombus with a multi-word label wraps) and gates many
   near-passes. `<br>` explicit breaks + markdown (`**bold**`) also here. (18%+.)
6. **Polish** — `<a>` wrapping for `click`/`href` nodes, `<title>`/`<desc>`.
