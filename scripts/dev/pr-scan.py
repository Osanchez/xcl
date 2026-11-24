#!/usr/bin/env python3
"""Read-only scanner for the open pull requests of an XCL repository.

  pr-scan.py report [--out DIR]                    HTML + Markdown + JSON report
  pr-scan.py covers [KEYWORDS...] [--files-from-branch [BASE]]
                                                   is this work already covered by a PR?

Only queries GitHub (through `gh api graphql`); it never comments, labels or reviews.
Python 3.9+, standard library only.
"""
from __future__ import annotations

import argparse
import datetime as dt
import html
import json
import os
import re
import subprocess
import sys

DEFAULT_REPO = "ivall/xcl"
DEFAULT_OUT = "reports/latest"
FILE_LIMIT = 100
STALE_DAYS = 7
MERGED_DAYS = 30
FILE_JACCARD = 0.5
TITLE_JACCARD = 0.6
HOT_MIN_PRS = 4
HOT_FRACTION = 0.05

TESTS_AREA = "Tests/e2e"
SRC = r"^crates/xcl/src/"
# Ordered: the first matching row wins for a path.
AREA_TABLE = [
    (re.compile(SRC + r"hle/d3d12x(/|\.|_|$)"), "D3D12.X"),
    (re.compile(SRC + r"gpu/recomp(/|\.|_|$)"), "Recompiler"),
    (re.compile(SRC + r"hle/xgameruntime(/|\.|_|$)"), "XGameRuntime"),
    (re.compile(SRC + r"hle/kernelx(/|\.|_|$)"), "kernelx"),
    (re.compile(SRC + r"hle/(xaudio2|xma2|opus)(/|\.|_|$)"), "XAudio2"),
    (re.compile(SRC + r"hle/dstorage(/|\.|_|$)"), "DirectStorage"),
    (re.compile(SRC + r"vfs(/|\.|_|$)"), "VFS"),
    (re.compile(SRC + r"gpu/uma(/|\.|_|$)"), "Memory"),
    (re.compile(SRC + r"tls(/|\.|_|$)"), "TLS"),
    (re.compile(SRC + r"(pe|loader|resolver)(/|\.|_|$)"), "PE loader"),
    (re.compile(SRC + r"shaders(/|\.|_|$)"), "Shaders"),
    (re.compile(SRC + r"frame(/|\.|_|$)"), "Frame clock"),
    (re.compile(SRC + r"(eh|hooks|host|launcher|main|preflight|report|config|manifest)(/|\.|_|$)"), "xcl run"),
    (re.compile(SRC + r"hle/(gameinput|kbm)(/|\.|_|$)"), "Input"),
    (re.compile(r"^(crates/xcl/tests/|tests/)"), TESTS_AREA),
    (re.compile(r"^scripts/"), "Scripts"),
    (re.compile(r"^README\.md$"), "README"),
    (re.compile(r"^(docs/|HANDOFF\.md$|THIRD_PARTY\.md$)"), "Docs"),
    (re.compile(r"^\.github/"), "CI"),
    (re.compile(SRC + r"gpu(/|$)"), "GPU"),
    (re.compile(SRC + r"hle/"), "HLE"),
    (re.compile(SRC), "Runtime"),
]
OTHER = "Other"
ALL_AREAS = [a for _, a in AREA_TABLE] + [OTHER]
_TITLE_ALIASES = {a.lower(): a for a in ALL_AREAS}
_TITLE_ALIASES.update({"tests": TESTS_AREA, "e2e": TESTS_AREA, "ci": "CI"})

STOPWORDS = frozenset("""a an and are as at be by for from in into is it its of on or that the this to
with when while without via use uses using add adds fix fixes fixed make makes not no only per
than then so""".split())


# ---------------------------------------------------------------- pure functions

def area_for_path(path):
    for rx, area in AREA_TABLE:
        if rx.search(path):
            return area
    return OTHER


def title_prefix_area(title):
    if ":" not in title:
        return None
    return _TITLE_ALIASES.get(title.split(":", 1)[0].strip().lower())


def area_lines(pr):
    """changed lines per area, from the file list"""
    out = {}
    for f in pr.get("files", []):
        a = area_for_path(f["path"])
        out[a] = out.get(a, 0) + f.get("additions", 0) + f.get("deletions", 0)
    return out


def areas_for_pr(pr):
    """(areas, primary): areas from paths and the title prefix; primary = most changed lines."""
    lines = area_lines(pr)
    tp = title_prefix_area(pr.get("title", ""))
    areas = set(lines)
    if tp:
        areas.add(tp)
    if lines:
        primary = sorted(lines.items(), key=lambda kv: (-kv[1], kv[0]))[0][0]
    else:
        primary = tp or OTHER
    if not areas:
        areas = {primary}
    return sorted(areas), primary


def title_tokens(title):
    if title_prefix_area(title):
        title = title.split(":", 1)[1]
    toks = re.findall(r"[a-z0-9][a-z0-9_.]*", title.lower())
    return frozenset(t.rstrip(".") for t in toks if t.rstrip(".") not in STOPWORDS)


def jaccard(a, b):
    a, b = set(a), set(b)
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def hot_threshold(n_prs):
    return max(HOT_MIN_PRS, HOT_FRACTION * n_prs)


def hot_files(prs, min_prs=HOT_MIN_PRS, fraction=HOT_FRACTION):
    """{path: sorted PR numbers} for files touched by more than max(min_prs, fraction * len(prs)) PRs."""
    touched = {}
    for p in prs:
        for path in {f["path"] for f in p.get("files", [])}:
            touched.setdefault(path, []).append(p["number"])
    limit = max(min_prs, fraction * len(prs))
    return {path: sorted(nums) for path, nums in touched.items() if len(nums) > limit}


def link_reason(a, b, hot=frozenset()):
    """Why two PRs are linked, or None. Dicts need 'file_set' (set of paths) and 'title'.
    Hot files are left out of the file Jaccard, and a file link needs a shared non-hot file."""
    fa, fb = a["file_set"] - hot, b["file_set"] - hot
    fs = jaccard(fa, fb) if fa & fb else 0.0
    if fs >= FILE_JACCARD:
        return {"kind": "shared files", "score": round(fs, 2)}
    ts = jaccard(title_tokens(a["title"]), title_tokens(b["title"]))
    if ts >= TITLE_JACCARD:
        return {"kind": "similar title", "score": round(ts, 2)}
    return None


def clusters(prs, hot=frozenset()):
    """Connected components (size >= 2) of the link graph, with the links that explain them.

    prs: dicts with number, title, files (list of {path,...}). Returns a list of
    {"members": [numbers], "links": [{"a","b","kind","score"}]}, largest first."""
    items = []
    for p in prs:
        q = dict(p)
        q["file_set"] = frozenset(f["path"] for f in p.get("files", []))
        items.append(q)
    parent = list(range(len(items)))

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    links = []
    for i in range(len(items)):
        for j in range(i + 1, len(items)):
            r = link_reason(items[i], items[j], hot)
            if r:
                links.append((i, j, r))
                parent[find(i)] = find(j)
    groups = {}
    for i in range(len(items)):
        groups.setdefault(find(i), []).append(i)
    out = []
    for root, idx in groups.items():
        if len(idx) < 2:
            continue
        out.append({
            "members": sorted(items[i]["number"] for i in idx),
            "links": [{"a": items[i]["number"], "b": items[j]["number"], **r}
                      for i, j, r in links if find(i) == root],
        })
    out.sort(key=lambda c: (-len(c["members"]), c["members"]))
    return out


def is_unreviewed(pr):
    return pr.get("reviewDecision") != "APPROVED" and pr.get("reviews", 0) == 0


def looks_done(pr):
    return (not pr.get("isDraft") and pr.get("ci") == "SUCCESS"
            and (is_unreviewed(pr) or pr.get("reviewDecision") == "APPROVED"))


def parse_time(s):
    return dt.datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)


def age_days(ts, now):
    return max(0, (now - parse_time(ts)).days)


def is_stale(pr, now):
    return (now - parse_time(pr["updatedAt"])).days >= STALE_DAYS


def review_state(pr):
    d = pr.get("reviewDecision")
    if d:
        return d
    n = pr.get("reviews", 0)
    return "unreviewed" if n == 0 else "reviewed (%d)" % n


def enrich(pr, now):
    areas, primary = areas_for_pr(pr)
    out = dict(pr)
    out.update({
        "areas": areas, "primary": primary,
        "unreviewed": is_unreviewed(pr), "looksDone": looks_done(pr),
        "stale": is_stale(pr, now), "ageDays": age_days(pr["createdAt"], now),
    })
    return out


def build_analysis(prs, now):
    prs = [enrich(p, now) for p in prs]
    per_area = {}
    for p in prs:
        per_area.setdefault(p["primary"], []).append(p)
    hot = hot_files(prs)
    hot_set = frozenset(hot)
    area_clusters = {a: clusters(ps, hot_set) for a, ps in per_area.items() if len(ps) > 1}
    area_clusters = {a: c for a, c in area_clusters.items() if c}
    return {
        "prs": prs,
        "areas": per_area,
        "clusters": clusters(prs, hot_set),
        "hotFiles": [{"file": f, "count": len(n), "prs": n}
                     for f, n in sorted(hot.items(), key=lambda kv: (-len(kv[1]), kv[0]))],
        "hotThreshold": hot_threshold(len(prs)),
        "areaClusters": area_clusters,
    }


def score_cover(pr, keywords, files):
    """(score, why) of an open/merged PR against keywords and/or a file list."""
    score, why = 0, []
    title, body = pr["title"].lower(), (pr.get("body") or "").lower()
    paths = [f["path"] for f in pr.get("files", [])]
    for k in keywords:
        k = k.lower()
        if k in title:
            score += 3
            why.append("title:" + k)
        if k in body:
            score += 1
            why.append("body:" + k)
        hit = [p for p in paths if k in p.lower()]
        if hit:
            score += 2
            why.append("file:%s (%d)" % (k, len(hit)))
    shared = sorted(set(paths) & set(files))
    if shared:
        score += 3 * len(shared)
        why.append("shares %d file(s): %s" % (len(shared), ", ".join(shared[:3]) + (", ..." if len(shared) > 3 else "")))
    return score, why


# ---------------------------------------------------------------- rendering (pure)

def esc(s):
    return html.escape(str(s), quote=True)


def safe_url(u):
    return u if isinstance(u, str) and u.startswith("https://github.com/") else "#"


def md_esc(s):
    return re.sub(r"([\\`*_{}\[\]<>|#])", r"\\\1", str(s)).replace("\n", " ")


def slug(area):
    return "area-" + re.sub(r"[^a-z0-9]+", "-", area.lower()).strip("-")


def pr_files_note(p):
    n = p.get("filesTotal", len(p.get("files", [])))
    return "%d+" % len(p["files"]) if p.get("filesTruncated") else str(n)


CSS = """
:root{--bg:#fff;--fg:#1f2328;--muted:#656d76;--line:#d0d7de;--card:#f6f8fa;--link:#0969da;--ok:#1a7f37;--bad:#cf222e;--warn:#9a6700}
@media (prefers-color-scheme:dark){:root{--bg:#0d1117;--fg:#e6edf3;--muted:#8d96a0;--line:#30363d;--card:#161b22;--link:#4493f8;--ok:#3fb950;--bad:#f85149;--warn:#d29922}}
*{box-sizing:border-box}
body{margin:0;padding:1rem;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif}
main{max-width:1100px;margin:0 auto}
a{color:var(--link);text-decoration:none}a:hover{text-decoration:underline}
h1{font-size:1.6rem;margin:.2rem 0 1rem}h2{font-size:1.2rem;margin:2rem 0 .5rem;border-bottom:1px solid var(--line);padding-bottom:.25rem}
h3{font-size:1rem;margin:1.2rem 0 .3rem}
.cards{display:flex;flex-wrap:wrap;gap:.6rem}.card{background:var(--card);border:1px solid var(--line);border-radius:6px;padding:.5rem .9rem;min-width:7rem}
.card b{display:block;font-size:1.4rem}.card span,.muted{color:var(--muted)}
.tw{overflow-x:auto}table{border-collapse:collapse;width:100%}th,td{text-align:left;padding:.3rem .6rem;border-bottom:1px solid var(--line);vertical-align:top}
th{font-size:.85rem;color:var(--muted)}td.n,th.n{text-align:right;white-space:nowrap}
.ok{color:var(--ok)}.bad{color:var(--bad)}.warn{color:var(--warn)}
ul{padding-left:1.2rem}footer{margin-top:2rem;color:var(--muted);font-size:.85rem}
@media (max-width:600px){body{padding:.5rem}th,td{padding:.25rem .4rem}}
"""


def ci_class(s):
    return {"SUCCESS": "ok", "FAILURE": "bad", "ERROR": "bad"}.get(s, "warn")


def cluster_why(c):
    parts = []
    for l in c["links"][:6]:
        parts.append("#%d~#%d %s %.2f" % (l["a"], l["b"], l["kind"], l["score"]))
    if len(c["links"]) > 6:
        parts.append("+%d more" % (len(c["links"]) - 6))
    return "; ".join(parts)


def render_html(an, repo, now):
    prs = an["prs"]
    by_num = {p["number"]: p for p in prs}

    def plink(p):
        return '<a href="%s">#%d</a>' % (esc(safe_url(p["url"])), p["number"])

    def prow(p):
        also = [a for a in p["areas"] if a != p["primary"]]
        title = esc(p["title"]) + (' <span class="muted">[draft]</span>' if p["isDraft"] else "")
        if also:
            title += ' <span class="muted">(also %s)</span>' % esc(", ".join(also))
        return ("<tr><td>%s</td><td>%s</td><td>%s</td><td class=n>%dd</td><td>%s</td>"
                "<td class=%s>%s</td><td class=n>%s</td></tr>") % (
            plink(p), title, esc(p["author"]), p["ageDays"], esc(review_state(p)),
            ci_class(p["ci"]), esc(p["ci"] or "none"), esc(pr_files_note(p)))

    head = "<tr><th>PR<th>Title<th>Author<th class=n>Age<th>Review<th>CI<th class=n>Files</tr>"
    o = []
    o.append("<!doctype html><html lang=en><head><meta charset=utf-8>"
             "<meta name=viewport content='width=device-width,initial-scale=1'>"
             "<title>PR scan: %s</title><style>%s</style></head><body><main>" % (esc(repo), CSS))
    o.append("<h1>Open pull requests: %s</h1>" % esc(repo))
    n_un = sum(p["unreviewed"] for p in prs)
    cards = [("Open", len(prs)), ("Unreviewed", n_un), ("Draft", sum(p["isDraft"] for p in prs)),
             ("Looks done", sum(p["looksDone"] for p in prs)), ("Stale (%dd+)" % STALE_DAYS, sum(p["stale"] for p in prs)),
             ("Overlap clusters", len(an["clusters"]))]
    o.append('<div class="cards">%s</div>' % "".join(
        "<div class=card><b>%d</b><span>%s</span></div>" % (v, esc(k)) for k, v in cards))

    o.append("<h2>Areas</h2><div class=tw><table><tr><th>Area<th class=n>Open<th class=n>Unreviewed<th class=n>Draft<th class=n>+ lines<th class=n>- lines</tr>")
    areas = sorted(an["areas"], key=lambda a: (-len(an["areas"][a]), a))
    for a in areas:
        ps = an["areas"][a]
        o.append("<tr><td><a href='#%s'>%s</a><td class=n>%d<td class=n>%d<td class=n>%d<td class=n>%d<td class=n>%d</tr>" % (
            slug(a), esc(a), len(ps), sum(p["unreviewed"] for p in ps), sum(p["isDraft"] for p in ps),
            sum(p["additions"] for p in ps), sum(p["deletions"] for p in ps)))
    o.append("</table></div>")
    o.append("<p class=muted>Each PR is counted once, under its primary area (the one with the most changed lines).</p>")

    o.append("<h2>Hot files</h2><p class=muted>Files touched by more than %g open PRs: conflict hotspots. They are left out of the overlap clusters.</p>" % an["hotThreshold"])
    if not an["hotFiles"]:
        o.append("<p class=muted>None.</p>")
    else:
        o.append("<div class=tw><table><tr><th>File<th class=n>Open PRs<th>PRs</tr>")
        for h in an["hotFiles"]:
            o.append("<tr><td>%s<td class=n>%d<td>%s</tr>" % (
                esc(h["file"]), h["count"], ", ".join(plink(by_num[n]) for n in h["prs"])))
        o.append("</table></div>")

    o.append("<h2>Pull requests by area</h2>")
    for a in areas:
        ps = sorted(an["areas"][a], key=lambda p: p["number"])
        o.append("<h3 id='%s'>%s <span class=muted>(%d)</span></h3>" % (slug(a), esc(a), len(ps)))
        o.append("<div class=tw><table>%s%s</table></div>" % (head, "".join(prow(p) for p in ps)))
        for c in an["areaClusters"].get(a, []):
            o.append("<p class=muted>Overlap within %s: %s (%s)</p>" % (
                esc(a), ", ".join("#%d" % n for n in c["members"]), esc(cluster_why(c))))

    o.append("<h2>Overlap clusters</h2>")
    if not an["clusters"]:
        o.append("<p class=muted>None.</p>")
    else:
        o.append("<p class=muted>PRs linked by changed files (Jaccard &ge; %.1f) or title words (Jaccard &ge; %.1f), across all PRs.</p>" % (FILE_JACCARD, TITLE_JACCARD))
        for c in an["clusters"]:
            o.append("<h3>%s</h3><p class=muted>%s</p><ul>" % (
                ", ".join(plink(by_num[n]) for n in c["members"]), esc(cluster_why(c))))
            for n in c["members"]:
                o.append("<li>%s %s <span class=muted>(%s)</span></li>" % (
                    plink(by_num[n]), esc(by_num[n]["title"]), esc(by_num[n]["primary"])))
            o.append("</ul>")

    def simple_list(title, sel, note):
        o.append("<h2>%s</h2><p class=muted>%s</p>" % (esc(title), esc(note)))
        if not sel:
            o.append("<p class=muted>None.</p>")
            return
        o.append("<div class=tw><table>%s%s</table></div>" % (head, "".join(prow(p) for p in sorted(sel, key=lambda p: p["number"]))))

    simple_list("Looks done", [p for p in prs if p["looksDone"]],
                "Not draft, CI green, and unreviewed or approved: ready for a look.")
    simple_list("Stale", [p for p in prs if p["stale"]], "No update in %d days or more." % STALE_DAYS)
    o.append("<footer>Generated %s UTC for %s. Read-only scan of public PR metadata.</footer></main></body></html>" % (
        now.strftime("%Y-%m-%d %H:%M"), esc(repo)))
    return "".join(o)


def render_md(an, repo, now):
    prs = an["prs"]
    by_num = {p["number"]: p for p in prs}

    def plink(p):
        return "[#%d](%s)" % (p["number"], safe_url(p["url"]))

    def prow(p):
        t = md_esc(p["title"]) + (" (draft)" if p["isDraft"] else "")
        return "| %s | %s | %s | %dd | %s | %s | %s |" % (
            plink(p), t, md_esc(p["author"]), p["ageDays"], md_esc(review_state(p)), md_esc(p["ci"] or "none"), pr_files_note(p))

    head = ["| PR | Title | Author | Age | Review | CI | Files |", "|---|---|---|---|---|---|---|"]
    o = ["# Open pull requests: %s" % repo, ""]
    o.append("Open %d, unreviewed %d, draft %d, looks done %d, stale %d, overlap clusters %d." % (
        len(prs), sum(p["unreviewed"] for p in prs), sum(p["isDraft"] for p in prs),
        sum(p["looksDone"] for p in prs), sum(p["stale"] for p in prs), len(an["clusters"])))
    o += ["", "## Areas", "", "| Area | Open | Unreviewed | Draft | + lines | - lines |", "|---|---|---|---|---|---|"]
    areas = sorted(an["areas"], key=lambda a: (-len(an["areas"][a]), a))
    for a in areas:
        ps = an["areas"][a]
        o.append("| [%s](#%s) | %d | %d | %d | %d | %d |" % (md_esc(a), slug(a), len(ps), sum(p["unreviewed"] for p in ps),
                 sum(p["isDraft"] for p in ps), sum(p["additions"] for p in ps), sum(p["deletions"] for p in ps)))
    o += ["", "## Hot files", "", "Files touched by more than %g open PRs: conflict hotspots. They are left out of the overlap clusters." % an["hotThreshold"], ""]
    if an["hotFiles"]:
        o += ["| File | Open PRs | PRs |", "|---|---|---|"]
        o += ["| %s | %d | %s |" % (md_esc(h["file"]), h["count"], ", ".join(plink(by_num[n]) for n in h["prs"])) for h in an["hotFiles"]]
    else:
        o.append("None.")
    o += ["", "## Pull requests by area", ""]
    for a in areas:
        ps = sorted(an["areas"][a], key=lambda p: p["number"])
        o += ['<a id="%s"></a>' % slug(a), "", "### %s (%d)" % (md_esc(a), len(ps)), ""] + head + [prow(p) for p in ps] + [""]
        for c in an["areaClusters"].get(a, []):
            o += ["Overlap within %s: %s (%s)" % (md_esc(a), ", ".join("#%d" % n for n in c["members"]), md_esc(cluster_why(c))), ""]
    o += ["## Overlap clusters", ""]
    if not an["clusters"]:
        o += ["None.", ""]
    for c in an["clusters"]:
        o += ["- " + ", ".join(plink(by_num[n]) for n in c["members"]) + " : " + md_esc(cluster_why(c))]
        o += ["  - %s %s (%s)" % (plink(by_num[n]), md_esc(by_num[n]["title"]), md_esc(by_num[n]["primary"])) for n in c["members"]]
    for title, sel in (("Looks done", [p for p in prs if p["looksDone"]]), ("Stale", [p for p in prs if p["stale"]])):
        o += ["", "## " + title, ""]
        if sel:
            o += head + [prow(p) for p in sorted(sel, key=lambda p: p["number"])]
        else:
            o.append("None.")
    o += ["", "Generated %s UTC for %s." % (now.strftime("%Y-%m-%d %H:%M"), repo), ""]
    return "\n".join(o)


# ---------------------------------------------------------------- I/O (queries only)

PR_FIELDS = """
number title url body isDraft createdAt updatedAt state mergedAt reviewDecision
author { login }
labels(first: 20) { nodes { name } }
additions deletions
files(first: %d) { totalCount nodes { path additions deletions } }
reviews { totalCount }
commits(last: 1) { nodes { commit { statusCheckRollup { state } } } }
""" % FILE_LIMIT

OPEN_QUERY = """
query($owner: String!, $name: String!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 50, after: $cursor) {
      pageInfo { hasNextPage endCursor }
      nodes { %s }
    }
  }
}""" % PR_FIELDS

SEARCH_QUERY = """
query($q: String!, $cursor: String) {
  search(query: $q, type: ISSUE, first: 50, after: $cursor) {
    pageInfo { hasNextPage endCursor }
    nodes { ... on PullRequest { %s } }
  }
}""" % PR_FIELDS


def gh_graphql(query, variables):
    cmd = ["gh", "api", "graphql", "-f", "query=" + query]
    for k, v in variables.items():
        if v is not None:
            cmd += ["-f", "%s=%s" % (k, v)]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
    except FileNotFoundError:
        sys.exit("pr-scan: the `gh` CLI is required (https://cli.github.com)")
    if r.returncode != 0:
        sys.exit("pr-scan: gh failed: " + (r.stderr.strip() or r.stdout.strip()))
    data = json.loads(r.stdout)
    if data.get("errors"):
        sys.exit("pr-scan: GraphQL errors: " + json.dumps(data["errors"])[:500])
    return data["data"]


def normalize(n):
    commits = (n.get("commits") or {}).get("nodes") or []
    rollup = None
    if commits and commits[0].get("commit", {}).get("statusCheckRollup"):
        rollup = commits[0]["commit"]["statusCheckRollup"]["state"]
    files = [f for f in (n.get("files") or {}).get("nodes") or [] if f]
    total = (n.get("files") or {}).get("totalCount", len(files))
    return {
        "number": n["number"], "title": n["title"], "url": n["url"], "body": n.get("body") or "",
        "author": (n.get("author") or {}).get("login") or "ghost",
        "isDraft": bool(n.get("isDraft")), "state": "MERGED" if n.get("mergedAt") else n.get("state", "OPEN"),
        "createdAt": n["createdAt"], "updatedAt": n["updatedAt"], "mergedAt": n.get("mergedAt"),
        "reviewDecision": n.get("reviewDecision"), "reviews": (n.get("reviews") or {}).get("totalCount", 0),
        "labels": [l["name"] for l in (n.get("labels") or {}).get("nodes") or []],
        "additions": n.get("additions", 0), "deletions": n.get("deletions", 0),
        "files": files, "filesTotal": total, "filesTruncated": total > len(files),
        "ci": rollup,
    }


def fetch_paged(query, base_vars, getter):
    out, cursor = [], None
    while True:
        data = gh_graphql(query, dict(base_vars, cursor=cursor))
        conn = getter(data)
        out += [normalize(n) for n in conn["nodes"] if n and "number" in n]
        if not conn["pageInfo"]["hasNextPage"]:
            return out
        cursor = conn["pageInfo"]["endCursor"]


def split_repo(repo):
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repo):
        sys.exit("pr-scan: --repo must be OWNER/NAME")
    return repo.split("/")


def fetch_open(repo):
    owner, name = split_repo(repo)
    return fetch_paged(OPEN_QUERY, {"owner": owner, "name": name}, lambda d: d["repository"]["pullRequests"])


def fetch_merged(repo, now):
    since = (now - dt.timedelta(days=MERGED_DAYS)).strftime("%Y-%m-%d")
    q = "repo:%s is:pr is:merged merged:>=%s" % (repo, since)
    split_repo(repo)
    return fetch_paged(SEARCH_QUERY, {"q": q}, lambda d: d["search"])


def write(path, text):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def public_pr(p):
    return {k: v for k, v in p.items() if k != "body"}


def cmd_report(args):
    now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    an = build_analysis(fetch_open(args.repo), now)
    os.makedirs(args.out, exist_ok=True)
    write(os.path.join(args.out, "index.html"), render_html(an, args.repo, now))
    write(os.path.join(args.out, "report.md"), render_md(an, args.repo, now))
    data = {
        "repo": args.repo, "generatedAt": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "prs": [public_pr(p) for p in an["prs"]], "clusters": an["clusters"], "areaClusters": an["areaClusters"],
        "hotFiles": an["hotFiles"], "hotThreshold": an["hotThreshold"],
    }
    write(os.path.join(args.out, "prs.json"), json.dumps(data, indent=2, ensure_ascii=False) + "\n")
    summary = {
        "repo": args.repo, "open": len(an["prs"]), "out": args.out, "clusters": len(an["clusters"]),
        "areas": {a: len(ps) for a, ps in sorted(an["areas"].items())},
    }
    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        print("%s: %d open PRs, %d overlap clusters, written to %s" % (args.repo, summary["open"], summary["clusters"], args.out))
        for a, n in sorted(summary["areas"].items(), key=lambda kv: (-kv[1], kv[0])):
            print("  %-14s %d" % (a, n))
    return 0


def branch_files(base):
    r = subprocess.run(["git", "diff", "--name-only", "%s...HEAD" % base], capture_output=True, text=True, encoding="utf-8")
    if r.returncode != 0:
        sys.exit("pr-scan: git diff failed: " + r.stderr.strip())
    return [l.strip() for l in r.stdout.splitlines() if l.strip()]


def cmd_covers(args):
    if not args.keywords and args.files_from_branch is None:
        sys.exit("pr-scan: covers needs KEYWORDS and/or --files-from-branch [BASE]")
    files = branch_files(args.files_from_branch) if args.files_from_branch is not None else []
    if args.files_from_branch is not None and not files and not args.keywords:
        print("no changed files in %s...HEAD" % args.files_from_branch)
        return 0
    now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    prs = fetch_open(args.repo) + fetch_merged(args.repo, now)
    hits = []
    for p in prs:
        s, why = score_cover(p, args.keywords, files)
        if s > 0:
            hits.append({"score": s, "number": p["number"], "state": p["state"], "title": p["title"],
                         "url": p["url"], "why": why})
    hits.sort(key=lambda h: (-h["score"], h["state"] != "OPEN", -h["number"]))
    if args.json:
        print(json.dumps(hits, indent=2))
    elif not hits:
        print("no open or recently merged PR matches")
    else:
        for h in hits:
            print("%3d  #%d  %-6s  %s\n       %s\n       why: %s" % (h["score"], h["number"], h["state"], h["title"], h["url"], "; ".join(h["why"])))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="Read-only scan of the open PRs of an XCL repository.")
    ap.add_argument("--repo", default=DEFAULT_REPO, help="OWNER/NAME (default %(default)s)")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("report", help="write index.html, report.md and prs.json")
    r.add_argument("--out", default=DEFAULT_OUT, help="output directory (default %(default)s)")
    r.set_defaults(fn=cmd_report)
    c = sub.add_parser("covers", help="find open / recently merged PRs that cover some work")
    c.add_argument("keywords", nargs="*")
    c.add_argument("--files-from-branch", nargs="?", const="main", default=None, metavar="BASE",
                   help="match the files of `git diff --name-only BASE...HEAD` (BASE default main)")
    c.set_defaults(fn=cmd_covers)
    # let --repo / --json also come after the subcommand
    for s in (r, c):
        s.add_argument("--repo", default=argparse.SUPPRESS, help=argparse.SUPPRESS)
        s.add_argument("--json", action="store_true", default=argparse.SUPPRESS, help=argparse.SUPPRESS)
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
