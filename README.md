# carcimaid

A pure-Rust renderer that turns [mermaid](https://github.com/mermaid-js/mermaid)
diagrams into SVG — no browser, no Node.

> *carcinisation, but for diagrams: everything eventually renders to a crab.*

## Why

mermaid is the de-facto diagram-as-text format, but rendering it requires a
headless browser (mermaid runs in the DOM). carcimaid aims to render mermaid to
SVG natively in Rust, suitable for embedding in build tools, docs pipelines, and
servers without spinning up Chromium.

## Approach: compliance-driven development

We treat the official mermaid CLI as an **oracle**. For every diagram in a test
corpus we render twice — with carcimaid and with mermaid-cli — and **structurally
diff** the two SVGs (element tree + classes + text exactly; numeric geometry
within a tolerance). The gap drives development: we grow the parser, layout, and
renderer until the structural diff shrinks to within tolerance.

See [`ATTRIBUTION.md`](ATTRIBUTION.md) for the grammar, layout algorithm, and
corpus this work derives from, all used under their original licenses.

## Workspace layout

| Crate | Path | What it is |
|-------|------|-----------|
| `carcimaid` | `crates/carcimaid` | The library: `parse → ir → layout → render → svg`. |
| `carcimaid-cli` | `crates/carcimaid-cli` | CLI front-end (binary `carcimaid`). |
| `compliance` | `crates/compliance` | Harness: runs the oracle + carcimaid and diffs the SVGs. |
| `roughr` | `crates/roughr` | Rust port of the rough.js subset mermaid uses for its hand-drawn look. |

The corpus lives in [`corpus/`](corpus/), organised by diagram type.

## Usage

```sh
# Render a diagram to SVG
cargo run -p carcimaid-cli -- input.mmd -o out.svg
# or via stdin/stdout
echo 'flowchart TD
  A[Start] --> B[End]' | cargo run -p carcimaid-cli

# Run the compliance suite (needs Docker/Podman for the oracle)
cargo run -p compliance -- --corpus corpus --artifacts artifacts
# carcimaid-only (skip the oracle):
cargo run -p compliance -- --no-oracle
```

## Visual comparison viewer

After a compliance run (which writes `<artifacts>/<case-id>/{oracle,carcimaid}.svg`
plus a diff report per case), build a self-contained browser page to compare
every corpus diagram side by side:

```sh
cargo run -p compliance -- --artifacts artifacts/corpus
python3 tools/build_viewer.py            # writes artifacts/viewer.html
```

Open `artifacts/viewer.html` in any browser (no server needed — everything is
inlined). Features:

- **mermaid (oracle) vs carcimaid** rendered side by side, with our pane given a
  mermaid-like theme so unstyled geometry is legible.
- **Next/Prev** buttons and <kbd>←</kbd>/<kbd>→</kbd> keys; filter by diagram
  type and status (diffs/passes/errors), jump to any case by id, sort (by id /
  most / fewest diffs), and a fit-width toggle.
- Each view shows the **case ID** (with a copy button) — use it to give feedback
  on a specific diagram. A per-case notes box saves to `localStorage`, and
  **export feedback** downloads all notes as JSON.
- The source `.mmd` and the structural diff are shown below the renders.

## The oracle (mermaid-cli)

The harness shells out to the official mermaid CLI via its Docker image so the
reference renderer is reproducible. On this NixOS host it runs under rootless
podman; the harness invokes it as `--user 0:0`. One-time host setup, if the
image won't pull/run under rootless podman:

```sh
# ~/.config/containers/policy.json
{ "default": [{"type": "insecureAcceptAnything"}] }

# ~/.config/containers/storage.conf  (single-UID hosts with no /etc/subuid)
[storage]
driver = "overlay"
[storage.options.overlay]
ignore_chown_errors = "true"

docker pull docker.io/minlag/mermaid-cli:latest
```

## Status

Early but real: two diagram types render, the other 21 are corpus-only so far.

- **Corpus**: ~1,100 diagrams covering all 23 mermaid diagram types, harvested
  from mermaid 11.15.0's own cypress specs and demos (MIT, per-file provenance
  in each `corpus/<type>/mermaid/SOURCES.tsv`), plus a few hand-written seeds.
- **Flowchart** (most mature): parser covers directions, the v11 shape catalog
  (`@{shape: …}`, including the rough.js hand-drawn `look` via `roughr`), edge
  chains/styles/labels, `A & B` groups, subgraphs with per-subgraph direction,
  `style`/`classDef`/`class`/`:::`/`linkStyle`, and YAML frontmatter config
  (`nodeSpacing`/`rankSpacing`, themes). Layout runs the real dagre algorithm
  (via the `dagre` crate) with mermaid's parameters and measures text with the
  same font the oracle resolves to (DejaVu Sans), so node coordinates, edge
  `curveBasis` routing, and the viewBox match mermaid exactly on supported
  features. The renderer emits mermaid's `htmlLabels:false` SVG DOM
  element-for-element.
- **Sequence** (in progress): participants/actors (UML shapes, boxes,
  create/destroy), all message arrow types, activations, `autonumber`, notes,
  `loop`/`alt`/`opt`/`par`/`rect` blocks, text wrapping, multi-line labels.
- **Everything else**: harvested corpus and oracle plumbing are in place; no
  parser/renderer yet.

A growing share of corpus diagrams already match the oracle's SVG structurally
diff-for-diff; run the compliance suite for current numbers.

Not production-ready.

## License

MIT — see [`LICENSE`](LICENSE). Bundled third-party material (mermaid-derived
corpus and SVG fragments, the rough.js port, the DejaVu Sans font) is
inventoried with full license texts in [`ATTRIBUTION.md`](ATTRIBUTION.md).
