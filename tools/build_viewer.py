#!/usr/bin/env python3
"""Build a self-contained HTML viewer to compare corpus renders side by side.

Reads the compliance artifacts (mermaid oracle SVG + carcimaid SVG + report per
case) and the corpus sources, and writes one standalone HTML file with next/prev
navigation. Each view shows the case ID (for feedback), the two SVGs side by
side, the source, and the structural diff.

Usage:
    python3 tools/build_viewer.py [artifacts-dir] [corpus-dir] [out.html]
Defaults: artifacts/mermaid  corpus/flowchart/mermaid  artifacts/viewer.html
"""
import json
import re
import sys
from pathlib import Path

artifacts = Path(sys.argv[1] if len(sys.argv) > 1 else "artifacts/mermaid")
corpus = Path(sys.argv[2] if len(sys.argv) > 2 else "corpus/flowchart/mermaid")
out = Path(sys.argv[3] if len(sys.argv) > 3 else "artifacts/viewer.html")


def read(p: Path) -> str:
    try:
        return p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


# Provenance from the corpus manifest, if present.
sources = {}
man = corpus / "SOURCES.tsv"
if man.exists():
    for line in man.read_text().splitlines()[1:]:
        parts = line.split("\t")
        if len(parts) >= 2:
            sources[parts[0].removesuffix(".mmd")] = parts[1]

cases = []
for d in sorted(p for p in artifacts.iterdir() if p.is_dir()):
    cid = d.name
    ours = read(d / "carcimaid.svg")
    if not ours:
        continue
    # Rename our svg's id prefix so the oracle's embedded `#my-svg{…}` styles
    # (and duplicate ids) don't leak across the two panels. Internal marker
    # references use the same prefix, so a blanket replace stays consistent.
    ours = ours.replace("my-svg", "c-svg")
    oracle = read(d / "oracle.svg")
    report = read(d / "report.txt")
    m = re.search(r"differences:\s*(\d+)", report)
    diffs = int(m.group(1)) if m else None
    m = re.search(r"tag_similarity:\s*([\d.]+)", report)
    sim = float(m.group(1)) if m else None
    # The diff detail lines (after the blank line following the header).
    detail = report.split("\n\n", 1)[1] if "\n\n" in report else ""
    status = "oracle-err" if not oracle else ("pass" if diffs == 0 else "diff")
    cases.append(
        {
            "id": cid,
            "source": read(corpus / f"{cid}.mmd"),
            "origin": sources.get(cid, ""),
            "oracle": oracle,
            "ours": ours,
            "diffs": diffs,
            "sim": sim,
            "detail": detail.strip(),
            "status": status,
        }
    )

payload = json.dumps(cases, ensure_ascii=False).replace("</", "<\\/")

HTML = """<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>carcimaid — corpus comparison</title>
<style>
  :root { --pass:#2e7d32; --diff:#c26401; --err:#b00020; --bg:#f5f5f7; --line:#ddd; }
  * { box-sizing: border-box; }
  body { margin:0; font:14px/1.45 -apple-system,Segoe UI,Roboto,sans-serif; color:#222; background:var(--bg); }
  header { position:sticky; top:0; z-index:5; background:#fff; border-bottom:1px solid var(--line);
           padding:10px 16px; display:flex; gap:12px; align-items:center; flex-wrap:wrap; }
  header .id { font-weight:700; font-size:18px; font-family:ui-monospace,Menlo,monospace; }
  header .pos { color:#666; }
  .badge { padding:2px 9px; border-radius:12px; color:#fff; font-weight:600; font-size:12px; }
  .badge.pass{background:var(--pass)} .badge.diff{background:var(--diff)} .badge.err{background:var(--err)}
  button, select { font:inherit; padding:5px 10px; border:1px solid #bbb; border-radius:6px; background:#fff; cursor:pointer; }
  button:hover { background:#eee; }
  .spacer { flex:1; }
  main { padding:16px; }
  .panes { display:grid; grid-template-columns:1fr 1fr; gap:16px; }
  .pane { background:#fff; border:1px solid var(--line); border-radius:8px; overflow:hidden; }
  .pane h2 { margin:0; padding:8px 12px; font-size:13px; background:#fafafa; border-bottom:1px solid var(--line); }
  .pane .body { padding:12px; overflow:auto; max-height:70vh; text-align:center; background:#fff;
                background-image:linear-gradient(45deg,#f0f0f0 25%,transparent 25%,transparent 75%,#f0f0f0 75%),
                                 linear-gradient(45deg,#f0f0f0 25%,transparent 25%,transparent 75%,#f0f0f0 75%);
                background-size:16px 16px; background-position:0 0,8px 8px; }
  .fit .pane .body svg { max-width:100%; height:auto; }
  .missing { color:var(--err); padding:20px; }
  /* Both SVGs now carry their own <style> (oracle's is mermaid's; ours is a
     focused mirror), so the panes are themed by the SVG content itself. */
  .cols { display:grid; grid-template-columns:1fr 1fr; gap:16px; margin-top:16px; }
  pre { margin:0; padding:12px; background:#fff; border:1px solid var(--line); border-radius:8px;
        overflow:auto; max-height:30vh; font:12px/1.4 ui-monospace,Menlo,monospace; white-space:pre-wrap; }
  .section h3 { margin:0 0 6px; font-size:12px; text-transform:uppercase; letter-spacing:.04em; color:#888; }
  .origin { color:#888; font-size:12px; }
  textarea { width:100%; min-height:70px; font:13px/1.4 inherit; padding:8px; border:1px solid #bbb; border-radius:6px; }
  kbd { background:#eee; border:1px solid #ccc; border-bottom-width:2px; border-radius:4px; padding:0 5px; font-size:11px; }
</style></head>
<body>
<header>
  <button id="prev">&larr; Prev</button>
  <button id="next">Next &rarr;</button>
  <span class="pos" id="pos"></span>
  <span class="id" id="cid"></span>
  <button id="copy" title="Copy case ID">copy id</button>
  <span class="badge" id="badge"></span>
  <span class="origin" id="origin"></span>
  <span class="spacer"></span>
  <label>filter <select id="filter">
    <option value="all">all</option><option value="diff">diffs only</option>
    <option value="pass">passes only</option><option value="err">oracle errors</option>
  </select></label>
  <label>sort <select id="sort">
    <option value="id">by id</option><option value="diffs">most diffs</option><option value="least">fewest diffs</option>
  </select></label>
  <label><input type="checkbox" id="fit" checked> fit width</label>
  <button id="export">export feedback</button>
  <span style="color:#888">&larr;/&rarr; to navigate</span>
</header>
<main>
  <div class="panes" id="panes">
    <div class="pane"><h2>mermaid (oracle)</h2><div class="body" id="oracle"></div></div>
    <div class="pane"><h2>carcimaid</h2><div class="body" id="ours"></div></div>
  </div>
  <div class="cols">
    <div class="section"><h3>source (.mmd)</h3><pre id="source"></pre></div>
    <div class="section"><h3>structural diff</h3><pre id="detail"></pre></div>
  </div>
  <div class="section" style="margin-top:16px">
    <h3>feedback for <span id="fbid" style="font-family:ui-monospace,monospace"></span> (saved locally)</h3>
    <textarea id="fb" placeholder="Notes about this rendering…"></textarea>
  </div>
</main>
<script id="data" type="application/json">__DATA__</script>
<script>
const ALL = JSON.parse(document.getElementById('data').textContent);
const $ = id => document.getElementById(id);
let filter='all', sort='id', idx=0;

function view() {
  let v = ALL.filter(c => filter==='all' ? true
        : filter==='pass' ? c.status==='pass'
        : filter==='err' ? c.status==='oracle-err'
        : c.status==='diff');
  const key = c => (c.diffs==null ? 1e9 : c.diffs);
  if (sort==='diffs') v.sort((a,b)=>key(b)-key(a));
  else if (sort==='least') v.sort((a,b)=>key(a)-key(b));
  else v.sort((a,b)=>a.id<b.id?-1:1);
  return v;
}
function render() {
  const v = view();
  if (!v.length) { $('cid').textContent='(no cases match filter)'; return; }
  idx = (idx % v.length + v.length) % v.length;
  const c = v[idx];
  $('pos').textContent = `${idx+1} / ${v.length}`;
  $('cid').textContent = c.id;
  $('fbid').textContent = c.id;
  $('origin').textContent = c.origin;
  const b=$('badge'); b.className='badge '+(c.status==='pass'?'pass':c.status==='oracle-err'?'err':'diff');
  b.textContent = c.status==='pass' ? 'PASS' : c.status==='oracle-err' ? 'ORACLE ERROR'
                  : (c.diffs+' diffs · sim '+(c.sim??'?'));
  $('oracle').innerHTML = c.oracle || '<div class="missing">oracle did not render this diagram</div>';
  $('ours').innerHTML = c.ours || '<div class="missing">no output</div>';
  $('source').textContent = c.source || '(source not found)';
  $('detail').textContent = c.detail || (c.status==='pass' ? '✓ exact structural match' : '');
  $('fb').value = localStorage.getItem('cfb:'+c.id) || '';
  location.hash = encodeURIComponent(c.id);
}
function go(n){ idx+=n; render(); }
$('prev').onclick=()=>go(-1); $('next').onclick=()=>go(1);
$('filter').onchange=e=>{filter=e.target.value; idx=0; render();};
$('sort').onchange=e=>{sort=e.target.value; idx=0; render();};
$('fit').onchange=e=>document.body.classList.toggle('fit', e.target.checked);
$('copy').onclick=()=>navigator.clipboard.writeText(view()[idx].id);
$('fb').oninput=e=>localStorage.setItem('cfb:'+view()[idx].id, e.target.value);
$('export').onclick=()=>{
  const out={}; for(let i=0;i<localStorage.length;i++){const k=localStorage.key(i);
    if(k.startsWith('cfb:')&&localStorage.getItem(k).trim()) out[k.slice(4)]=localStorage.getItem(k);}
  const blob=new Blob([JSON.stringify(out,null,2)],{type:'application/json'});
  const a=document.createElement('a'); a.href=URL.createObjectURL(blob); a.download='carcimaid-feedback.json'; a.click();
};
document.addEventListener('keydown',e=>{
  if(e.target.tagName==='TEXTAREA'||e.target.tagName==='SELECT') return;
  if(e.key==='ArrowLeft')go(-1); else if(e.key==='ArrowRight')go(1);
});
document.body.classList.add('fit');
// Deep-link to a case id via #hash.
if(location.hash){const want=decodeURIComponent(location.hash.slice(1));const v=view();const i=v.findIndex(c=>c.id===want);if(i>=0)idx=i;}
render();
</script>
</body></html>
"""

out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(HTML.replace("__DATA__", payload), encoding="utf-8")
n_pass = sum(1 for c in cases if c["status"] == "pass")
print(f"wrote {out} — {len(cases)} cases ({n_pass} pass)")
