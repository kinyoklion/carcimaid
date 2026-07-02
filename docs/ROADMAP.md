# Roadmap

Priorities are derived from the compliance baseline — the structural diff of
carcimaid's SVG against the mermaid CLI across the 199-diagram flowchart corpus.
Run it yourself with `cargo run -p compliance -- --corpus corpus/flowchart/mermaid`.

## Baseline snapshot (flowchart corpus)

Latest (3c29f51):

- 199 diagrams, 196 compared (3 unrenderable by the mermaid CLI itself).
- **Exact passes: 22.** ≤5 diffs: 33; ≤10 diffs: 63.
- The comparator now aligns children by key (LCS over `tag|class|id`) so an
  inserted/omitted element no longer cascades and masks real diffs — feedback is
  now nuanced. Parser skips `accTitle`/`accDescr` and handles v11 `@{shape,label}`.

Earlier (cc6ce70):

- Exact passes: 19. Of the ~177 non-passing, **13 explicitly request the ELK
  layout engine** (`config: layout: elk`) — a different engine we don't
  implement, so effectively out of scope. Achievable denominator ≈ 183.
- The single-fix passes are harvested (no diagrams sit at 1-2 diffs); remaining
  cases each need multiple fixes.

Earlier milestone (a50bb49):

- Exact passes 14; ≤5 diffs: 30; ≤10 diffs: 48.
- Geometry is correct (node coords, edge curves, viewBox, clusters,
  rhombus/circle); labels wrap into per-word `tspan` rows; `classDef`/`class`/
  `style`/`linkStyle`/`click` directives and `:::class` suffixes no longer
  create phantom nodes.
- Known limits: long single-line labels can miss by ~1px (our advance-width sum
  runs ~0.04px/char under mermaid's browser `getBBox`); remaining unsupported
  features below.

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
5. **Label wrapping + per-word tspans** — DONE (91d1ade). Greedy wrap to width
   200; one outer `row` tspan per line, one inner tspan per word.
6. **Directives** — DONE (a50bb49). Skip `classDef`/`class`/`style`/`linkStyle`/
   `click` and strip `:::class`; was the biggest single unlock (passes 6→14).

## Remaining (long tail, by diff frequency)

- **More node shapes** — path family (stadium/cylinder → `<path>`, 39 diffs) and
  polygon family (hexagon/trapezoid/parallelogram/subroutine → `<polygon>`, 33)
  still render as `rect`. Polygon family is calibratable like rhombus; path
  family (bezier/arc `d`) is hard.
- **`<a>` link wrapping** — `click`/`href` nodes: mermaid wraps the node group in
  `<a>` (45 diffs).
- **`g` vs `rect` (81)** — largest tag mismatch; needs investigation (likely
  subgraph node emission ordering / nested clusters).
- **`<br>` breaks + markdown** (`**bold**`) labels.
- **ELK layout** (13 diagrams request `layout: elk`) — a separate engine
  (elkjs); out of scope unless an ELK port is undertaken. Consider marking these
  in the harness so pass-rate reflects the dagre-achievable corpus.
- **Edge-label positioning** (surfaced once the diff stopped cascading): our
  `g.edgeLabel` groups render at `translate(0,0)` instead of the edge midpoint,
  and their background rects are unsized. A real bug to fix next.
- **`<title>`/`<desc>` + aria** from `accTitle`/`accDescr` (currently skipped
  entirely; mermaid emits them and sets `aria-labelledby`/`aria-describedby`).
- **~1px width drift**: our advance-width sum runs ~0.04px/char under mermaid's
  browser `getBBox`; caps long-label passes. Fundamental metric limit.
