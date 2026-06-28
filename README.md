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

Early. Flowcharts are the first target. The parser handles directions, the
common node shapes, edge chains, edge styles, and `|label|` edge labels; layout
is a placeholder layered algorithm pending a dagre-compatible port; SVG output
is being aligned to mermaid's actual DOM structure. Not production-ready.

## License

MIT — see [`LICENSE`](LICENSE).
