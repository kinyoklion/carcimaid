#!/usr/bin/env python3
"""Harvest flowchart diagram sources from a mermaid checkout into the corpus.

Usage:
    python3 tools/harvest_corpus.py <mermaid-checkout> <out-dir>

Extracts mermaid `flowchart`/`graph` definitions from mermaid's cypress
rendering specs (backtick template literals, skipping ${...}-interpolated ones)
and from the `demos/*.html` files (`<pre class="mermaid">` blocks), dedupes
them, writes one `.mmd` per diagram, and records provenance in `SOURCES.tsv`.

mermaid is MIT-licensed (Copyright (c) 2014 - 2022 Knut Sveidqvist); the
harvested diagrams are vendored under that license. See `corpus/ATTRIBUTION.md`.
"""
import html
import re
import sys
from pathlib import Path


def is_flowchart(text: str) -> bool:
    """A flowchart/graph diagram with at least one statement after the header
    (skips degenerate fixtures like a bare `flowchart`)."""
    lines = text.splitlines()
    i = 0
    if i < len(lines) and lines[i].strip() == "---":
        i += 1
        while i < len(lines) and lines[i].strip() != "---":
            i += 1
        i += 1
    body = [l.strip() for l in lines[i:] if l.strip() and not l.strip().startswith("%%")]
    if not body:
        return False
    head = body[0].split()
    if (head[0] if head else "") not in ("flowchart", "graph"):
        return False
    # Require a real body: either more lines, or a header line that also carries
    # statements (e.g. `graph TD; A-->B`).
    return len(body) > 1 or len(head) > 2 or ";" in body[0]


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
    for m in re.finditer(
        r'<(?:pre|div)[^>]*class="[^"]*mermaid[^"]*"[^>]*>(.*?)</(?:pre|div)>',
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
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    mermaid, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)

    sources = [
        *sorted(mermaid.glob("cypress/integration/rendering/flowchart/*.spec.js")),
        *sorted(mermaid.glob("cypress/integration/rendering/flowchart/*.spec.ts")),
        *sorted(mermaid.glob("demos/flowchart*.html")),
        *sorted(mermaid.glob("demos/dataflowchart.html")),
    ]

    seen, manifest, count = set(), [], 0
    for path in sources:
        text = path.read_text(encoding="utf-8", errors="replace")
        extractor = extract_html if path.suffix == ".html" else extract_backticks
        rel = path.relative_to(mermaid).as_posix()
        for block_idx, raw in enumerate(extractor(text)):
            diagram = dedent(raw)
            if not diagram or not is_flowchart(diagram):
                continue
            key = diagram.strip()
            if key in seen:
                continue
            seen.add(key)
            stem = path.name.replace(".spec.js", "").replace(".spec.ts", "").replace(
                ".html", ""
            )
            if "demos/" in rel:
                stem = "demo-" + stem
            name = f"{stem}_{block_idx:03d}.mmd"
            (out / name).write_text(diagram + "\n", encoding="utf-8")
            manifest.append((name, rel, block_idx))
            count += 1

    with (out / "SOURCES.tsv").open("w", encoding="utf-8") as f:
        f.write("file\tsource\tblock\n")
        for name, rel, idx in sorted(manifest):
            f.write(f"{name}\t{rel}\t{idx}\n")

    print(f"wrote {count} diagrams to {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
