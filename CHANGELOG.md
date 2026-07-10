# Changelog

## [0.1.4](https://github.com/kinyoklion/carcimaid/compare/carcimaid-v0.1.3...carcimaid-v0.1.4) (2026-07-10)

### Features

* **sequence:** honor the theme override in sequence diagrams

## [0.1.3](https://github.com/kinyoklion/carcimaid/compare/carcimaid-v0.1.2...carcimaid-v0.1.3) (2026-07-10)

### Features

* **api:** let callers override the theme via RenderOptions

## [0.1.2](https://github.com/kinyoklion/carcimaid/compare/carcimaid-v0.1.1...carcimaid-v0.1.2) (2026-07-10)

### Features

* **api:** add render_to_svg_with(source, Background) knob
* **sequence:** honor @{ "alias": … } participant metadata

### Bug Fixes

* **compliance:** exit 0 when the harness completes

## 0.1.1 (2026-07-08)

### Bug Fixes

* **sequence:** render empty label lines as zero-width space

## 0.1.0

Initial release baseline. Pure-Rust mermaid-diagram → SVG renderer with
flowchart and sequence-diagram support (developed against a structural +
visual diff of the mermaid CLI). Subsequent versions are cut by Synthase from
Conventional-Commit history.
