# Corpus attribution

The compliance corpus is organised by diagram type (e.g. `corpus/flowchart/`).
Each `.mmd` file is one test case. Provenance:

## Hand-written seed cases (carcimaid, MIT)
The seed cases currently in `corpus/flowchart/` (`simple-chain`, `decision`,
`shapes`, `lr-edge-styles`) are original, written for this project and covered
by the repository's MIT license. They exist to exercise the parser/layout/render
subset as it grows.

## mermaid-derived cases (MIT, under `*/mermaid/`)
Larger coverage comes from diagram source harvested from the mermaid project,
which is MIT-licensed (`Copyright (c) 2014 - 2022 Knut Sveidqvist`). These live
in a `mermaid/` subdirectory per type (currently `corpus/flowchart/mermaid/`,
200 diagrams) so their origin is unambiguous. Sources:

- `cypress/integration/rendering/flowchart/*.spec.{js,ts}` — backtick template
  literals (interpolated `${...}` ones are skipped).
- `demos/flowchart*.html` — `<pre class="mermaid">` blocks.

The harvested `.mmd` files are kept **byte-faithful** to upstream (no injected
comments, so they round-trip through both renderers unchanged). Provenance for
every file — upstream path and block index — is recorded in the generated
`SOURCES.tsv` manifest alongside them. Regenerate with:

```sh
git clone --depth 1 https://github.com/mermaid-js/mermaid.git
python3 tools/harvest_corpus.py mermaid corpus/flowchart/mermaid
```

The full MIT notice is reproduced in the repository root `ATTRIBUTION.md`.

> Note: mermaid does **not** publish reusable baseline images (its visual
> regression baselines live in a private Argos cloud account). We therefore do
> not redistribute any mermaid images — reference SVGs are generated locally by
> running the mermaid CLI oracle.
