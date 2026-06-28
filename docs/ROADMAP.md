# Roadmap

Priorities are derived from the compliance baseline — the structural diff of
carcimaid's SVG against the mermaid CLI across the 199-diagram flowchart corpus.
Run it yourself with `cargo run -p compliance -- --corpus corpus/flowchart/mermaid`.

## Baseline snapshot (flowchart corpus)

After text measurement (commit f112648):

- 199 diagrams, 196 compared (3 are unrenderable by the mermaid CLI itself).
- Structural tag-similarity: median **0.98**, 80% ≥ 0.95.
- Exact passes: **1** (`graph TD; A[Christmas]` — first single-node match).
- Per-case diffs: median **24**; cases within 10 diffs: **37/196** (was 24).

Diff composition now (occurrences across all cases):

| rank | diff | cause | fix |
|------|------|-------|-----|
| 1 | `transform` (992) | node/edge-label positions | **dagre layout** |
| 2 | `d` (862) | edge bezier routing | **dagre layout** |
| 3 | `width`/`height`/`x`/`y` (~1660) | multi-line & wrapped labels, non-rect shapes | label wrapping; per-shape sizing |
| 4 | `ChildCountMismatch` (692) | subgraphs, multi-line/markdown labels | clusters; label rows |
| 5 | `TextMismatch` (402) | markdown/`<br>`/KaTeX labels kept literal | label processing |
| 6 | `TagMismatch` (279) | unsupported shapes, `<a>` links | more shapes; link wrapping |

Single-line plain-rect node sizing now matches mermaid (e.g. simple-chain went
13 → 4 diffs, viewBox matches); the remaining geometry gap is positional.

## Next milestones, in impact order

1. **Text measurement** — DONE (commit f112648). Measures with DejaVu Sans (the
   font mermaid's headless Chromium resolves to) via `ttf-parser`; node width =
   text width + shape padding. Fixed single-line rect sizing + first exact pass.
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
