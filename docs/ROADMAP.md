# Roadmap

carcimaid is a pure-Rust mermaid-diagram → SVG renderer, developed against a
structural diff of its output versus the mermaid CLI (the "oracle", mermaid
**11.15.0**, `htmlLabels:false` / `useMaxWidth:false` / `securityLevel:loose`).

**Goal:** render *all* mermaid diagram types, growing compatibility incrementally
against a complete corpus. Progress is measured two ways, both necessary:

- **Structural** — `cargo run -p compliance -- --corpus ./corpus [--filter …]`
  diffs the element trees (tag/class/id/attrs). **Exact passes (0 diffs) is the
  north-star**; tag-similarity is a secondary signal. The comparator ignores
  `<style>` text, so it can report ~1.0 while the render looks wrong.
- **Visual** — render both sides to PNG and compare (the comparator misses
  colour/stroke/font/wrapping, all CSS-driven). The `compliance` run also emits
  `artifacts/corpus/**` which `tools/build_viewer.py` bundles into
  `artifacts/viewer.html` (type filter + jump-to-id). Always regenerate the
  viewer before ending a work session.

## Status

The corpus is **1125 cases across 23 diagram types** (harvested from mermaid's
cypress specs + demos by `tools/harvest_corpus.py`). Two types are implemented:

| type | cases | exact passes |
|------|-------|--------------|
| flowchart | 208 | 27 |
| sequence  | 156 | 22 |
| **total compared** | 364 | **49** |

The other 21 types (git 131, class 111, state 81, er 59, xychart 56, gantt 50,
block 46, … down to zenuml 3) are harvested but not yet rendered.

## Flowchart

Geometry is correct where element trees line up: dagre layout, `curveBasis`
edge routing, viewBox, clusters, and the calibrated node shapes match the oracle
exactly. Text is measured with DejaVu Sans via `ttf-parser` (mermaid's headless
Chromium resolves its font stack to DejaVu); labels wrap into per-word `tspan`
rows; `classDef`/`class`/`style`/`linkStyle`/`click` directives and `:::class`
suffixes are handled. `look: handDrawn` shapes render via the `roughr` crate
(rough.js port) — non-deterministic in mermaid, so `d` can't byte-match.

The bottleneck is **structural coverage, not geometry**: once a diagram uses a
feature we don't model, child counts diverge and everything downstream compares
against the wrong element. Remaining flowchart gaps, by corpus frequency:

- **Exotic node shapes** — polygon family (hexagon/trapezoid/parallelogram/
  subroutine) is calibratable like rhombus/circle; path family (stadium/cylinder)
  uses bezier/arc `d` (harder).
- **`<a>` link wrapping** — `click`/`href` nodes get wrapped in `<a>`.
- **Edge-label positioning** — `g.edgeLabel` groups need to sit at the edge
  midpoint with sized background rects.
- **ELK layout** — ~13 diagrams request `layout: elk`, a separate engine; out of
  scope unless an ELK port is undertaken (mark them so pass-rate reflects the
  dagre-achievable corpus).
- **~1px width drift** — advance-width sum runs ~0.04px/char under mermaid's
  browser `getBBox`; a fundamental metric limit that caps some long-label passes.

## Sequence

Implemented end-to-end (`parser/layout/render/seq_defs`, single ordered event
walk sharing one vertical cursor; DOM order follows mermaid's `.lower()`). The
full mermaid sequence CSS is emitted verbatim so renders match visually.

Done: `participant`/`actor` (stick figure) declarations · `box` groupings (with
title-driven width) · all message arrow types incl. the 16 directional arrows
(`-|/` solid / `-//` stick + reverse/dotted) · notes (left/right/over) · blocks
(loop/alt/opt/par, sections) with depth-proportional nested margins · rect
background regions (nested, drawn outer-first) · activations (`+`/`-` bars) ·
create/destroy participants · autonumber · title · multi-line (`<br>`) labels ·
`#entity;` escapes · `:wrap:`/`:nowrap:` wrapping of notes, messages, and
participant labels · the six `@{type}` UML shapes (boundary/control/entity/
database/queue/collections — visual approximations of mermaid's `getBBox`-based
geometry).

Remaining:

- **loop/opt condition auto-wrap to box width** — needs a loop-width *pre-pass*
  (mermaid's `calculateLoopBounds`): the block-label height is required at the
  block's start, but the box extent is only known at its end.
- **KaTeX math labels** (`$$…$$`) — out of scope; needs a LaTeX math engine
  (mermaid bundles KaTeX). ReX is the pure-Rust option if visual math is wanted.
- **`link` popup menus** (`forceMenus`) — niche.
- **`@{type}` shape geometry** — currently visual approximations; exact match
  needs mermaid's browser-`getBBox` label positioning.

## Next diagram types

By corpus size the highest-leverage next targets are **class (111)**,
**state (81)**, and **git (131)**. Each is a fresh renderer dispatched from the
`Diagram` enum, following the sequence pattern (parser → IR → layout → render,
verified structurally then visually across *all* corpus cases of that type).
