# Roadmap

Priorities are derived from the compliance baseline — the structural diff of
carcimaid's SVG against the mermaid CLI across the 199-diagram flowchart corpus.
Run it yourself with `cargo run -p compliance -- --corpus corpus/flowchart/mermaid`.

## Baseline snapshot (flowchart corpus)

- 199 diagrams, 196 compared (3 are unrenderable by the mermaid CLI itself).
- Structural tag-similarity: median **0.98**, 80% ≥ 0.95.
- Exact passes: **0** — every case still has numeric geometry diffs (node
  widths and the viewBox differ by more than the 1px tolerance).

Diff composition (occurrences across all cases):

| rank | diff | cause | fix |
|------|------|-------|-----|
| 1 | `d`, `transform`, `x`, `y` (~2000) | layout coordinates | **dagre layout** |
| 2 | `width`, `viewBox`, label-rect `x/y/w/h` (~1500) | node sizing | **text metrics** |
| 3 | `ChildCountMismatch` (684) | subgraphs, multi-line/markdown labels | clusters; label rows |
| 4 | `TextMismatch` (385) | markdown/`<br>`/KaTeX labels kept literal | label processing |
| 5 | `TagMismatch` (278) | unsupported shapes (`g`/`polygon`/`path` vs `rect`), `<a>` links | more shapes; link wrapping |

## Next milestones, in impact order

1. **Text measurement** — the single biggest lever and a prerequisite for any
   exact pass. Need font metrics (e.g. embed metrics for mermaid's default
   `trebuchet ms`/sans stack, or a metrics crate) to compute node box widths,
   label rect sizes, and the diagram viewBox the way mermaid does.
2. **Dagre layout** — port/vendor a dagre-compatible layered layout (rank →
   order → coordinate assignment) plus edge bezier routing, so `transform`/`d`
   match for multi-node graphs. See `ATTRIBUTION.md`.
3. **Node shape coverage** — extend beyond the current 5 shapes to mermaid's
   full set (hexagon, trapezoids, cylinder, subroutine, etc.), matching the
   `polygon`/`path` elements mermaid emits.
4. **Subgraphs / clusters** — parse `subgraph … end` and render the `g.clusters`
   group; resolves a large share of `ChildCountMismatch`.
5. **Rich labels** — markdown (`**bold**`), `<br>` line breaks, and multi-row
   text (mermaid emits one `tspan` row per line).
6. **Polish** — `<a>` wrapping for `click`/`href` nodes, `<title>`/`<desc>`
   accessibility nodes.
