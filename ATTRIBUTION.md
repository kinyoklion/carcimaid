# Attribution

carcimaid is developed for behavioural compliance with [mermaid][mermaid] and
derives from several open-source projects. This file records every external
work we reference, reuse, or compare against, and the license under which we do
so. carcimaid itself is MIT-licensed (see `LICENSE`).

## Reference & comparison

### mermaid — MIT
- Repository: <https://github.com/mermaid-js/mermaid>
- License: MIT — `Copyright (c) 2014 - 2022 Knut Sveidqvist`
- How we use it:
  - **Oracle**: the official mermaid CLI (`@mermaid-js/mermaid-cli`, run via the
    `minlag/mermaid-cli` Docker image) renders the reference SVG that our
    compliance harness diffs against.
  - **Grammar reference**: our flowchart parser follows the syntax defined in
    `packages/mermaid/src/diagrams/flowchart/parser/flow.jison` and its
    `*.spec.js` feature tests.
  - **SVG structure reference**: our renderer targets the DOM structure produced
    by mermaid's flowchart renderer (`flowRenderer-v3-unified.ts`,
    `rendering-elements/nodes.ts`, `edges.js`, `markers.js`).
  - **Corpus**: diagram source text harvested from mermaid's `cypress/` specs
    and `demos/` is vendored under `corpus/` with attribution — see
    `corpus/ATTRIBUTION.md`.

### dagre (dagrejs) — MIT
- Repository: <https://github.com/dagrejs/dagre>
- License: MIT
- The layered graph-layout algorithm mermaid uses for flowcharts. Our layout
  engine reimplements / ports this algorithm (rank → order → coordinate
  assignment).

### Layout crates evaluated as a basis for our dagre port
- `dagre` (kookyleo/dagre-rs) — Apache-2.0 — <https://github.com/kookyleo/dagre-rs>
  (a faithful Rust port of dagre.js; primary candidate to vendor/depend on).
- `rust-sugiyama` (paddison) — MIT — fallback Sugiyama layout core.
- `layout-rs` (nadavrot/layout) — MIT — referenced for SVG emission patterns.

### warpdotdev/mermaid-to-svg — MIT
- Repository: <https://github.com/warpdotdev/mermaid-to-svg>
- An existing pure-Rust mermaid→SVG implementation studied as an end-to-end
  architectural reference.

## Tooling dependencies
Rust crate dependencies (clap, roxmltree, …) are listed with their licenses in
the respective `Cargo.toml` files and resolved by Cargo; all are permissively
licensed (MIT/Apache-2.0).

[mermaid]: https://github.com/mermaid-js/mermaid
