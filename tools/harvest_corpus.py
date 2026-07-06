#!/usr/bin/env python3
"""Harvest mermaid diagram sources from a mermaid checkout into the corpus.

Usage:
    python3 tools/harvest_corpus.py <mermaid-checkout> <out-dir> [--only TYPE,TYPE]

Extracts mermaid diagram definitions from mermaid's cypress rendering specs
(backtick template literals, skipping ${...}-interpolated ones) and from the
`demos/*.html` files (`<pre class="mermaid">` blocks), detects each diagram's
type from its header keyword, dedupes, and writes one `.mmd` per diagram under
`<out-dir>/<type>/mermaid/`, recording provenance in a per-type `SOURCES.tsv`.

`--only TYPE,TYPE` restricts output to the given comma-separated type dirs (e.g.
`--only sequence,class`); use it to harvest new types without touching a
hand-curated `corpus/flowchart/`.

mermaid is MIT-licensed (Copyright (c) 2014 - 2022 Knut Sveidqvist); the
harvested diagrams are vendored under that license. See `corpus/ATTRIBUTION.md`.
"""
import html
import re
import sys
from pathlib import Path

# Map a diagram's leading header keyword (lowercased, trailing `:` stripped) to
# the corpus type directory it belongs in. Covers the mermaid 11.x diagram
# headers; several have `-v2`/`-beta` suffixes or C4 sub-variants.
TYPE_BY_KEYWORD = {
    "graph": "flowchart",
    "flowchart": "flowchart",
    "flowchart-elk": "flowchart",
    "sequencediagram": "sequence",
    "classdiagram": "class",
    "classdiagram-v2": "class",
    "statediagram": "state",
    "statediagram-v2": "state",
    "erdiagram": "er",
    "gantt": "gantt",
    "pie": "pie",
    "journey": "journey",
    "gitgraph": "git",
    "mindmap": "mindmap",
    "timeline": "timeline",
    "quadrantchart": "quadrant",
    "requirementdiagram": "requirement",
    "requirement": "requirement",
    "c4context": "c4",
    "c4container": "c4",
    "c4component": "c4",
    "c4dynamic": "c4",
    "c4deployment": "c4",
    "sankey-beta": "sankey",
    "sankey": "sankey",
    "xychart-beta": "xychart",
    "xychart": "xychart",
    "block-beta": "block",
    "block": "block",
    "packet-beta": "packet",
    "packet": "packet",
    "kanban": "kanban",
    "architecture-beta": "architecture",
    "architecture": "architecture",
    "radar-beta": "radar",
    "radar": "radar",
    "treemap": "treemap",
    "treemap-beta": "treemap",
    "zenuml": "zenuml",
    "info": "info",
}


def significant_lines(text: str):
    """The diagram's body lines, skipping YAML frontmatter, `%%{init}%%`
    directives, and `%%` comments/blank lines."""
    lines = text.splitlines()
    i = 0
    while i < len(lines) and not lines[i].strip():
        i += 1
    # Leading YAML frontmatter (`---` … `---`).
    if i < len(lines) and lines[i].strip() == "---":
        i += 1
        while i < len(lines) and lines[i].strip() != "---":
            i += 1
        i += 1
    return [s for l in lines[i:] if (s := l.strip()) and not s.startswith("%%")]


def detect_type(text: str):
    """Return the corpus type dir for `text`, or None if its header keyword is
    not a recognised diagram type (filters out non-diagram backtick blocks)."""
    body = significant_lines(text)
    if not body:
        return None
    head = body[0].split()
    if not head:
        return None
    kw = head[0].rstrip(":").lower()  # `gitGraph:` -> `gitgraph`
    typ = TYPE_BY_KEYWORD.get(kw)
    if typ is None:
        return None
    # Require a real body: more than the header alone, or a header line that
    # also carries statements (`graph TD; A-->B`).
    if len(body) > 1 or len(head) > 2 or ";" in body[0]:
        return typ
    return None


def extract_backticks(src: str):
    """Yield template-literal contents that contain no ${...} interpolation."""
    i, n = 0, len(src)
    while i < n:
        if src[i] == "`":
            j, buf = i + 1, []
            while j < n and src[j] != "`":
                if src[j] == "\\" and j + 1 < n:
                    buf.append(src[j : j + 2])
                    j += 2
                    continue
                buf.append(src[j])
                j += 1
            content = "".join(buf)
            if "${" not in content:
                yield content
            i = j + 1
        else:
            i += 1


def extract_html(src: str):
    """Yield diagram text from <pre/div class="mermaid"> blocks."""
    # Some demos format the closing tag with whitespace before `>`
    # (`</pre\n  >`), so allow `\s*` there — otherwise a diagram's block runs on
    # to a later `</pre>` and swallows several diagrams (plus stray tags).
    for m in re.finditer(
        r'<(?:pre|div)[^>]*class="[^"]*mermaid[^"]*"[^>]*>(.*?)</(?:pre|div)\s*>',
        src,
        re.S,
    ):
        yield html.unescape(m.group(1))


def dedent(text: str) -> str:
    lines = text.splitlines()
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    if not lines:
        return ""
    indent = min((len(l) - len(l.lstrip()) for l in lines if l.strip()), default=0)
    return "\n".join(l[indent:] for l in lines)


def main() -> int:
    positional = [a for a in sys.argv[1:] if not a.startswith("--")]
    only = None
    for i, a in enumerate(sys.argv[1:], start=1):
        if a == "--only" and i < len(sys.argv) - 1:
            only = {t.strip() for t in sys.argv[i + 1].split(",") if t.strip()}
            positional = [p for p in positional if p != sys.argv[i + 1]]
        elif a.startswith("--only="):
            only = {t.strip() for t in a[len("--only="):].split(",") if t.strip()}
    if len(positional) < 2:
        print(__doc__)
        return 2
    mermaid, out = Path(positional[0]), Path(positional[1])
    out.mkdir(parents=True, exist_ok=True)

    sources = [
        *sorted(mermaid.glob("cypress/integration/rendering/**/*.spec.js")),
        *sorted(mermaid.glob("cypress/integration/rendering/**/*.spec.ts")),
        *sorted(mermaid.glob("demos/*.html")),
    ]

    seen = set()
    manifests: dict[str, list] = {}
    counts: dict[str, int] = {}
    for path in sources:
        text = path.read_text(encoding="utf-8", errors="replace")
        extractor = extract_html if path.suffix == ".html" else extract_backticks
        rel = path.relative_to(mermaid).as_posix()
        for block_idx, raw in enumerate(extractor(text)):
            diagram = dedent(raw)
            if not diagram:
                continue
            typ = detect_type(diagram)
            if typ is None or (only is not None and typ not in only):
                continue
            key = diagram.strip()
            if key in seen:
                continue
            seen.add(key)
            stem = (
                path.name.replace(".spec.js", "").replace(".spec.ts", "").replace(".html", "")
            )
            if "demos/" in rel:
                stem = "demo-" + stem
            counts[typ] = counts.get(typ, 0) + 1
            name = f"{stem}_{block_idx:03d}.mmd"
            dest_dir = out / typ / "mermaid"
            dest_dir.mkdir(parents=True, exist_ok=True)
            (dest_dir / name).write_text(diagram + "\n", encoding="utf-8")
            manifests.setdefault(typ, []).append((name, rel, block_idx))

    total = 0
    for typ, manifest in sorted(manifests.items()):
        with (out / typ / "mermaid" / "SOURCES.tsv").open("w", encoding="utf-8") as f:
            f.write("file\tsource\tblock\n")
            for name, rel, idx in sorted(manifest):
                f.write(f"{name}\t{rel}\t{idx}\n")
        print(f"  {typ:14s} {len(manifest):4d}")
        total += len(manifest)

    print(f"wrote {total} diagrams across {len(manifests)} types to {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
