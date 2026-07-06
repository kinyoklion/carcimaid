#!/usr/bin/env python3
"""Build a self-contained HTML viewer to compare corpus renders side by side.

Walks the compliance artifacts tree (one dir per case, each holding the mermaid
oracle SVG + carcimaid SVG + report) and the corpus sources, and writes one
standalone HTML file. Cases are organised by diagram type (the first path
component of the case id, e.g. `sequence/mermaid/sequencediagram_012`); the
viewer can filter by type/status, jump to any case by id, and record feedback.

Usage:
    python3 tools/build_viewer.py [artifacts-dir] [corpus-dir] [out.html]
Defaults: artifacts/corpus  corpus  artifacts/viewer.html
"""
import json
import re
import sys
from pathlib import Path

artifacts = Path(sys.argv[1] if len(sys.argv) > 1 else "artifacts/corpus")
corpus = Path(sys.argv[2] if len(sys.argv) > 2 else "corpus")
out = Path(sys.argv[3] if len(sys.argv) > 3 else "artifacts/viewer.html")


def read(p: Path) -> str:
    try:
        return p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


# Provenance: load every per-type SOURCES.tsv under the corpus, keyed by the
# case's file stem (unique enough within a type; ids disambiguate across types).
sources = {}
for man in corpus.rglob("SOURCES.tsv"):
    for line in man.read_text(encoding="utf-8", errors="replace").splitlines()[1:]:
        parts = line.split("\t")
        if len(parts) >= 2:
            sources[parts[0].removesuffix(".mmd")] = parts[1]

# A case is any directory in the artifacts tree that holds a carcimaid.svg.
cases = []
for svg in sorted(artifacts.rglob("carcimaid.svg")):
    d = svg.parent
    cid = d.relative_to(artifacts).as_posix()
    dtype = cid.split("/", 1)[0]
    ours = read(svg)
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
    detail = report.split("\n\n", 1)[1] if "\n\n" in report else ""
    status = "oracle-err" if not oracle else ("pass" if diffs == 0 else "diff")
    stem = cid.rsplit("/", 1)[-1]
    cases.append(
        {
            "id": cid,
            "type": dtype,
            "source": read(corpus / f"{cid}.mmd"),
            "origin": sources.get(stem, ""),
            "oracle": oracle,
            "ours": ours,
            "diffs": diffs,
            "sim": sim,
            "detail": detail.strip(),
            "status": status,
        }
    )

types = sorted({c["type"] for c in cases})
payload = json.dumps(cases, ensure_ascii=False).replace("</", "<\\/")
type_opts = "".join(f'<option value="{t}">{t}</option>' for t in types)

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
  button, select, input[type=text] { font:inherit; padding:5px 10px; border:1px solid #bbb; border-radius:6px; background:#fff; }
  button { cursor:pointer; }
  button:hover { background:#eee; }
  #jump { width:240px; font-family:ui-monospace,Menlo,monospace; }
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
  .cols { display:grid; grid-template-columns:1fr 1fr; gap:16px; margin-top:16px; }
  pre { margin:0; padding:12px; background:#fff; border:1px solid var(--line); border-radius:8px;
        overflow:auto; max-height:30vh; font:12px/1.4 ui-monospace,Menlo,monospace; white-space:pre-wrap; }
  .section h3 { margin:0 0 6px; font-size:12px; text-transform:uppercase; letter-spacing:.04em; color:#888; }
  .origin { color:#888; font-size:12px; }
  textarea { width:100%; min-height:70px; font:13px/1.4 inherit; padding:8px; border:1px solid #bbb; border-radius:6px; }
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
  <label>type <select id="type"><option value="all">all types</option>__TYPES__</select></label>
  <label>filter <select id="filter">
    <option value="all">all</option><option value="diff">diffs only</option>
    <option value="pass">passes only</option><option value="err">oracle errors</option>
  </select></label>
  <label>sort <select id="sort">
    <option value="id">by id</option><option value="diffs">most diffs</option>
    <option value="least">fewest diffs</option><option value="worst">worst sim</option>
  </select></label>
  <label>jump <input type="text" id="jump" list="ids" placeholder="type a case id…"></label>
  <datalist id="ids"></datalist>
  <label><input type="checkbox" id="fit" checked> fit width</label>
  <button id="export">export feedback</button>
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
let dtype='all', filter='all', sort='id', idx=0;

// Populate the jump datalist with every case id.
$('ids').innerHTML = ALL.map(c=>`<option value="${c.id}">`).join('');

function view() {
  let v = ALL.filter(c => dtype==='all' ? true : c.type===dtype)
             .filter(c => filter==='all' ? true
        : filter==='pass' ? c.status==='pass'
        : filter==='err' ? c.status==='oracle-err'
        : c.status==='diff');
  const key = c => (c.diffs==null ? 1e9 : c.diffs);
  const skey = c => (c.sim==null ? 1e9 : c.sim);
  if (sort==='diffs') v.sort((a,b)=>key(b)-key(a));
  else if (sort==='least') v.sort((a,b)=>key(a)-key(b));
  else if (sort==='worst') v.sort((a,b)=>skey(a)-skey(b));
  else v.sort((a,b)=>a.id<b.id?-1:1);
  return v;
}
function render() {
  const v = view();
  if (!v.length) { $('cid').textContent='(no cases match filter)'; $('pos').textContent=''; return; }
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
// Jump to a specific case id (searches across ALL, switching type filter if needed).
function jumpTo(id){
  const target = ALL.find(c=>c.id===id);
  if(!target) return false;
  if(dtype!=='all' && target.type!==dtype){ dtype='all'; $('type').value='all'; }
  if(filter!=='all' && !( (filter==='pass'&&target.status==='pass')
      || (filter==='err'&&target.status==='oracle-err')
      || (filter==='diff'&&target.status==='diff') )){ filter='all'; $('filter').value='all'; }
  const i = view().findIndex(c=>c.id===id);
  if(i>=0){ idx=i; render(); return true; }
  return false;
}
$('prev').onclick=()=>go(-1); $('next').onclick=()=>go(1);
$('type').onchange=e=>{dtype=e.target.value; idx=0; render();};
$('filter').onchange=e=>{filter=e.target.value; idx=0; render();};
$('sort').onchange=e=>{sort=e.target.value; idx=0; render();};
$('jump').onchange=e=>{ if(jumpTo(e.target.value.trim())) e.target.blur(); };
$('fit').onchange=e=>document.body.classList.toggle('fit', e.target.checked);
$('copy').onclick=()=>navigator.clipboard.writeText(view()[idx].id);
$('fb').oninput=e=>localStorage.setItem('cfb:'+view()[idx].id, e.target.value);
$('export').onclick=()=>{
  const o={}; for(let i=0;i<localStorage.length;i++){const k=localStorage.key(i);
    if(k.startsWith('cfb:')&&localStorage.getItem(k).trim()) o[k.slice(4)]=localStorage.getItem(k);}
  const blob=new Blob([JSON.stringify(o,null,2)],{type:'application/json'});
  const a=document.createElement('a'); a.href=URL.createObjectURL(blob); a.download='carcimaid-feedback.json'; a.click();
};
document.addEventListener('keydown',e=>{
  if(e.target.tagName==='TEXTAREA'||e.target.tagName==='SELECT'||e.target.tagName==='INPUT') return;
  if(e.key==='ArrowLeft')go(-1); else if(e.key==='ArrowRight')go(1);
});
document.body.classList.add('fit');
// Deep-link to a case id via #hash.
if(location.hash){ jumpTo(decodeURIComponent(location.hash.slice(1))); } else render();
if(!$('cid').textContent) render();
</script>
</body></html>
"""

out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(HTML.replace("__DATA__", payload).replace("__TYPES__", type_opts), encoding="utf-8")
by_type = {}
for c in cases:
    by_type.setdefault(c["type"], [0, 0])
    by_type[c["type"]][0] += 1
    if c["status"] == "pass":
        by_type[c["type"]][1] += 1
n_pass = sum(1 for c in cases if c["status"] == "pass")
print(f"wrote {out} — {len(cases)} cases ({n_pass} pass) across {len(types)} types")
for t in types:
    print(f"  {t:12s} {by_type[t][0]:4d} cases  {by_type[t][1]:3d} pass")
