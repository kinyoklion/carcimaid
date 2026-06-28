# Corpus attribution

The compliance corpus is organised by diagram type (e.g. `corpus/flowchart/`).
Each `.mmd` file is one test case. Provenance:

## Hand-written seed cases (carcimaid, MIT)
The seed cases currently in `corpus/flowchart/` (`simple-chain`, `decision`,
`shapes`, `lr-edge-styles`) are original, written for this project and covered
by the repository's MIT license. They exist to exercise the parser/layout/render
subset as it grows.

## mermaid-derived cases (MIT, planned under `*/mermaid/`)
Larger coverage comes from diagram source harvested from the mermaid project,
which is MIT-licensed (`Copyright (c) 2014 - 2022 Knut Sveidqvist`). When added,
these live in a `mermaid/` subdirectory per type (e.g.
`corpus/flowchart/mermaid/`) so their origin is unambiguous. Sources:

- `cypress/integration/rendering/flowchart/*.spec.{js,ts}` — each `it(...)` block
  contains one inline diagram definition.
- `demos/flowchart*.html` — inline diagram definitions.

Harvested files retain a leading `%% source: <repo path>` comment pointing to the
exact upstream location. The full MIT notice is reproduced in the repository
root `ATTRIBUTION.md`.

> Note: mermaid does **not** publish reusable baseline images (its visual
> regression baselines live in a private Argos cloud account). We therefore do
> not redistribute any mermaid images — reference SVGs are generated locally by
> running the mermaid CLI oracle.
