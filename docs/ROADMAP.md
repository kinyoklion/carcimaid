# Roadmap

Priorities are derived from the compliance baseline — the structural diff of
carcimaid's SVG against the mermaid CLI across the 199-diagram flowchart corpus.
Run it yourself with `cargo run -p compliance -- --corpus corpus/flowchart/mermaid`.

## Baseline snapshot (flowchart corpus)

After text measurement + dagre layout (commit b6f2c39):

- 199 diagrams, 196 compared (3 are unrenderable by the mermaid CLI itself).
- **Exact passes: 5** (e.g. `simple-chain` — node coords, edge curves, viewBox
  all byte-for-byte). Geometry is now correct.
- Per-case diffs: median **24** (little aggregate change — see below).

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

## Next milestones, in impact order

1. **Text measurement** — DONE (f112648). DejaVu Sans via `ttf-parser`.
2. **Dagre layout + curveBasis edges** — DONE (b6f2c39). Drives the `dagre`
   crate with mermaid's params; edges via a d3-curveBasis reimplementation with
   4px arrow clipping. First exact passes.
3. **Subgraphs / clusters** — parse `subgraph … end`, model as dagre compound
   nodes, render the `g.clusters` group. Highest-frequency unsupported feature
   (31%); unblocks the biggest `ChildCountMismatch` cascades.
4. **Node shape coverage** — mermaid's full shape set (hexagon, trapezoids,
   cylinder, subroutine, …) as the `polygon`/`path` elements mermaid emits, with
   per-shape padding.
5. **Rich labels** — `<br>` line breaks and multi-row text (one `tspan` row per
   line), then markdown (`**bold**`).
6. **Polish** — `<a>` wrapping for `click`/`href` nodes, `<title>`/`<desc>`.
