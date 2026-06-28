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

Early. Flowcharts are the first target.

- **Corpus**: 199 flowchart diagrams harvested from mermaid's own cypress specs
  and demos (`corpus/flowchart/mermaid/`, MIT, provenance in `SOURCES.tsv`).
- **Parser**: directions, the common node shapes, edge chains, edge styles,
  `|label|` edge labels, `A & B` node groups, YAML frontmatter, and multibyte
  text. All 199 corpus diagrams parse and render to well-formed SVG.
- **Renderer**: emits mermaid's `htmlLabels:false` SVG DOM (root attrs, the 12
  arrowhead markers, `g.root` groups, `node.default`, `flowchart-link` edges,
  drop-shadow `defs`, `<text>/<tspan>` labels). The oracle is run with the same
  `htmlLabels:false` config so the two are comparable. Structural tag-similarity
  vs the mermaid CLI is ~1.0 and the rank-axis (vertical, for `TD`) coordinates
  already match mermaid exactly.
- **Layout**: placeholder layered algorithm. The remaining structural-diff gap
  is numeric — node widths/x-centering (needs real text metrics) and edge curve
  routing (needs dagre). That's the next milestone.

Not production-ready.

## License

MIT — see [`LICENSE`](LICENSE).
