#!/usr/bin/env python3
"""Read-only scanner for the open pull requests of an XCL repository.

  pr-scan.py report [--out DIR] [--history FILE]   HTML + Markdown + JSON report (+ trend history)
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
import math
import os
import re
import subprocess
import sys
import urllib.parse

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


# ---------------------------------------------------------------- history (pure)

HISTORY_MIN_GAP = dt.timedelta(minutes=30)
HISTORY_KEEP = dt.timedelta(days=90)
TREND_DAYS = 14
SPARK_DAYS = 7
TS_FMT = "%Y-%m-%dT%H:%M:%SZ"


def snapshot(an, now):
    """One history line for the current analysis."""
    prs = an["prs"]
    return {
        "ts": now.strftime(TS_FMT), "open": len(prs),
        "unreviewed": sum(p["unreviewed"] for p in prs), "draft": sum(bool(p["isDraft"]) for p in prs),
        "looksDone": sum(p["looksDone"] for p in prs), "clusters": len(an["clusters"]),
        "areas": {a: len(ps) for a, ps in sorted(an["areas"].items())},
        "prs": sorted(p["number"] for p in prs),
    }


def _int(v, default=0):
    return v if isinstance(v, int) and not isinstance(v, bool) else default


def parse_history(text):
    """Snapshots from JSON-lines text, oldest first. Blank, unparsable or malformed lines are skipped."""
    out = []
    for line in (text or "").splitlines():
        try:
            d = json.loads(line)
            parse_time(d["ts"])
        except (ValueError, KeyError, TypeError):
            continue
        if not isinstance(d, dict):
            continue
        areas = d.get("areas")
        prs = d.get("prs")
        out.append({
            "ts": d["ts"], "open": _int(d.get("open")), "unreviewed": _int(d.get("unreviewed")),
            "draft": _int(d.get("draft")), "looksDone": _int(d.get("looksDone")), "clusters": _int(d.get("clusters")),
            "areas": {str(k): _int(v) for k, v in areas.items()} if isinstance(areas, dict) else {},
            "prs": sorted(n for n in prs if isinstance(n, int) and not isinstance(n, bool)) if isinstance(prs, list) else None,
        })
    out.sort(key=lambda s: s["ts"])
    return out


def read_history(path):
    try:
        with open(path, encoding="utf-8") as f:
            return parse_history(f.read())
    except OSError:
        return []


def append_snapshot(hist, snap, min_gap=HISTORY_MIN_GAP):
    """hist + snap, unless the last line is less than min_gap older than snap."""
    if hist and parse_time(snap["ts"]) - parse_time(hist[-1]["ts"]) < min_gap:
        return list(hist)
    return list(hist) + [snap]


def prune_history(hist, now, keep=HISTORY_KEEP):
    return [s for s in hist if now - parse_time(s["ts"]) <= keep]


def history_text(hist):
    return "".join(json.dumps(s, separators=(",", ":"), ensure_ascii=False) + "\n" for s in hist)


def window(hist, now, days):
    return [s for s in hist if now - parse_time(s["ts"]) <= dt.timedelta(days=days)]


def new_and_closed(prev_prs, cur_prs):
    """(new, closed) counts between two sorted lists of open PR numbers."""
    a, b = set(prev_prs), set(cur_prs)
    return len(b - a), len(a - b)


def daily_summary(hist):
    """Per UTC day: last open / unreviewed, and PRs opened / closed between consecutive snapshots."""
    days = {}
    prev = None
    for s in hist:
        day = s["ts"][:10]
        d = days.setdefault(day, {"day": day, "open": 0, "unreviewed": 0, "new": 0, "closed": 0})
        d["open"], d["unreviewed"] = s["open"], s["unreviewed"]
        if prev is not None and prev["prs"] is not None and s["prs"] is not None:
            n, c = new_and_closed(prev["prs"], s["prs"])
            d["new"] += n
            d["closed"] += c
        prev = s
    return [days[k] for k in sorted(days)]


def change_24h(hist, key, current, now):
    """current - value in the newest snapshot at least 24 h old, or None without one."""
    base = [s for s in hist if now - parse_time(s["ts"]) >= dt.timedelta(hours=24)]
    return current - base[-1][key] if base else None


def downsample(vals, n):
    if len(vals) <= n:
        return list(vals)
    step = (len(vals) - 1) / (n - 1)
    return [vals[round(i * step)] for i in range(n)]


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


def ci_class(s):
    return {"SUCCESS": "ok", "FAILURE": "bad", "ERROR": "bad"}.get(s, "warn")


def cluster_why(c):
    parts = []
    for l in c["links"][:6]:
        parts.append("#%d~#%d %s %.2f" % (l["a"], l["b"], l["kind"], l["score"]))
    if len(c["links"]) > 6:
        parts.append("+%d more" % (len(c["links"]) - 6))
    return "; ".join(parts)


def bucket(p):
    """Mutually exclusive state of a PR for the area bars."""
    if p["isDraft"]:
        return "draft"
    if p["looksDone"]:
        return "done"
    return "wait" if p["unreviewed"] else "rev"


BUCKETS = [("draft", "Draft"), ("wait", "Waiting for review"), ("rev", "Reviewed"), ("done", "Looks done")]


def pill(text, kind="muted"):
    return '<span class="pill %s">%s</span>' % (kind, esc(text))


def ci_pill(ci):
    text = {"SUCCESS": "CI passing", "FAILURE": "CI failing", "ERROR": "CI failing", None: "CI none"}.get(ci, "CI pending")
    return pill(text, "ok" if ci == "SUCCESS" else "bad" if ci in ("FAILURE", "ERROR") else "muted" if ci is None else "warn")


def review_pill(p):
    d = p.get("reviewDecision")
    if d == "APPROVED":
        return pill("approved", "ok")
    if d == "CHANGES_REQUESTED":
        return pill("changes requested", "bad")
    return pill("unreviewed", "warn") if p["unreviewed"] else pill("reviewed", "accent")


def state_pills(p):
    out = [ci_pill(p["ci"]), review_pill(p)]
    if p["isDraft"]:
        out.append(pill("draft"))
    if p["looksDone"]:
        out.append(pill("looks done", "ok"))
    if p["stale"]:
        out.append(pill("stale", "warn"))
    return "".join(out)


def delta_text(d):
    if d is None:
        return ""
    if d == 0:
        return '<span class="delta">no change / 24 h</span>'
    return '<span class="delta">%s %d / 24 h</span>' % ("&#9650;" if d > 0 else "&#9660;", abs(d))


def tile(label, value, delta=None):
    return '<div class="stat"><div class="stat-l">%s</div><div class="stat-v">%d</div>%s</div>' % (esc(label), value, delta_text(delta))


def legend(items):
    return '<ul class="legend">%s</ul>' % "".join(
        '<li><i class="sw %s"></i>%s</li>' % (esc(cls), esc(name)) for name, cls in items)


def nice_ticks(maxv, n=4):
    """(step, top): an integer step and a top that is a multiple of it, covering maxv."""
    maxv = max(1, maxv)
    raw = maxv / n
    mag = 10 ** math.floor(math.log10(raw))
    step = max(1, math.ceil(next(m * mag for m in (1, 2, 2.5, 5, 10) if m * mag >= raw)))
    return step, step * math.ceil(maxv / step)


def _md(t):
    return "%s %d" % (t.strftime("%b"), t.day)


def svg_frame(w, h, label, body):
    return ('<svg class="chart" viewBox="0 0 %d %d" role="img" aria-label="%s" preserveAspectRatio="xMidYMid meet">'
            '<title>%s</title>%s</svg>' % (w, h, esc(label), esc(label), body))


def _y_axis(ml, mr, mt, mb, w, h, top, step):
    out = []
    for v in range(0, top + 1, step):
        y = mt + (h - mt - mb) * (1 - v / top)
        out.append('<line class="grid" x1="%d" x2="%d" y1="%.1f" y2="%.1f"/>'
                   '<text class="tick" x="%d" y="%.1f" text-anchor="end">%d</text>' % (ml, w - mr, y, y, ml - 6, y + 4, v))
    return "".join(out)


def svg_line_chart(series, label, w=480, h=240):
    """series: [{"name", "cls", "points": [(datetime, value)]}]. Empty text with no points."""
    pts = [p for s in series for p in s["points"]]
    if not pts:
        return '<p class="muted empty">No data yet.</p>'
    ml, mr, mt, mb = 36, 12, 12, 28
    t0 = min(t for t, _ in pts).timestamp()
    t1 = max(t for t, _ in pts).timestamp()
    step, top = nice_ticks(max(v for _, v in pts))
    span = t1 - t0

    def x(t):
        return (ml + (w - ml - mr) / 2) if span == 0 else ml + (w - ml - mr) * (t.timestamp() - t0) / span

    def y(v):
        return mt + (h - mt - mb) * (1 - v / top)

    body = [_y_axis(ml, mr, mt, mb, w, h, top, step)]
    t0d, t1d = dt.datetime.fromtimestamp(t0, dt.timezone.utc), dt.datetime.fromtimestamp(t1, dt.timezone.utc)
    if span < 2 * 86400:
        ticks = [(t0d + (t1d - t0d) * i / 2, "%02d:%02d" % ((t0d + (t1d - t0d) * i / 2).hour, (t0d + (t1d - t0d) * i / 2).minute)) for i in range(3)] if span else [(t0d, _md(t0d))]
    else:
        first = (t0d + dt.timedelta(days=1)).replace(hour=0, minute=0, second=0)
        every = max(1, math.ceil(span / 86400 / 6))
        ticks = []
        d = first
        while d <= t1d:
            ticks.append((d, _md(d)))
            d += dt.timedelta(days=every)
    for t, text in ticks:
        body.append('<text class="tick" x="%.1f" y="%d" text-anchor="middle">%s</text>' % (x(t), h - 8, esc(text)))
    for s in series:
        sp = s["points"]
        if not sp:
            continue
        if len(sp) > 1:
            body.append('<polyline class="line %s" points="%s"/>' % (
                esc(s["cls"]), " ".join("%.1f,%.1f" % (x(t), y(v)) for t, v in sp)))
        # tooltip dots: the last point of each day, plus the very last one
        marks = {t.date(): (t, v) for t, v in sp}
        for t, v in marks.values():
            body.append('<circle class="dot %s" cx="%.1f" cy="%.1f" r="%d"><title>%s</title></circle>' % (
                esc(s["cls"]), x(t), y(v), 4 if len(sp) == 1 else 3,
                esc("%s %s UTC: %s %d" % (_md(t), t.strftime("%H:%M"), s["name"], v))))
    return svg_frame(w, h, label, "".join(body))


def svg_bars(rows, label, w=480, h=240):
    """rows: [{"day", "new", "closed"}], grouped bars per day."""
    if not rows:
        return '<p class="muted empty">No data yet.</p>'
    ml, mr, mt, mb = 36, 12, 12, 28
    step, top = nice_ticks(max(max(r["new"], r["closed"]) for r in rows))
    pw = w - ml - mr
    gw = pw / max(len(rows), 4)
    bw = min(14, gw * 0.36)
    body = [_y_axis(ml, mr, mt, mb, w, h, top, step)]
    every = 1 if len(rows) <= 8 else 2
    for i, r in enumerate(rows):
        cx = ml + gw * (i + 0.5)
        for j, (key, cls, name) in enumerate((("new", "b-new", "opened"), ("closed", "b-closed", "closed"))):
            bh = (h - mt - mb) * r[key] / top
            bx = cx - bw - 1 + j * (bw + 2)
            body.append('<rect class="%s" x="%.1f" y="%.1f" width="%.1f" height="%.1f" rx="2"><title>%s</title></rect>' % (
                cls, bx, h - mb - bh, bw, max(bh, 0), esc("%s: %d %s" % (r["day"], r[key], name))))
        if (len(rows) - 1 - i) % every == 0:
            body.append('<text class="tick" x="%.1f" y="%d" text-anchor="middle">%s</text>' % (cx, h - 8, esc(r["day"][5:])))
    return svg_frame(w, h, label, "".join(body))


def sparkline(vals, label="", w=72, h=24):
    cls = 'class="spark" width="%d" height="%d" viewBox="0 0 %d %d"' % (w, h, w, h)
    if len(vals) < 2:
        return "<svg %s aria-hidden=\"true\"></svg>" % cls
    lo, hi = min(vals), max(vals)
    pts = " ".join("%.1f,%.1f" % (2 + (w - 4) * i / (len(vals) - 1),
                                  h / 2 if hi == lo else h - 3 - (h - 6) * (v - lo) / (hi - lo)) for i, v in enumerate(vals))
    return '<svg %s role="img" aria-label="%s"><title>%s</title><polyline class="spline" points="%s"/></svg>' % (
        cls, esc(label), esc(label), pts)


def avatar(login):
    return ('<img class="av" src="https://avatars.githubusercontent.com/%s?s=40" alt="%s" width="20" height="20" '
            'loading="lazy" decoding="async">' % (urllib.parse.quote(str(login), safe=""), esc(login)))


FONTS_URL = "https://fonts.googleapis.com/css2?family=Roboto+Condensed:wght@700&family=Roboto:wght@400;500;700&display=swap"

# Area colour pairs (light, dark); areas past the last pair cycle.
CATEGORY = [("#d81f82", "#ff7fc0"), ("#c96a12", "#ffb057"), ("#14839f", "#5fd0ea"), ("#4f7f16", "#b4e06a"),
            ("#7b6a78", "#cdb9c8"), ("#6a4cc7", "#b8a6ff"), ("#a8801a", "#e8cb62"), ("#c43b3b", "#ff9c94"),
            ("#2d7d5b", "#7fd9ae"), ("#8a4fb0", "#dcaef2")]


def category_css():
    light = "".join("--cat%d:%s;" % (i, c[0]) for i, c in enumerate(CATEGORY))
    dark = "".join("--cat%d:%s;" % (i, c[1]) for i, c in enumerate(CATEGORY))
    return light, dark


_LIGHT_CAT, _DARK_CAT = category_css()
_DARK_TOKENS = ("--page:#0e1219;--ink:#eef2f8;--soft:#a2adbc;--rule:#2a3240;--wash:#1a212c;--accent:#8ca8ff;--partial:#ffab5c;"
                "--good:#b4e06a;--bad:#ff8a80;" + _DARK_CAT)
CSS = (":root{color-scheme:light dark;--page:#fff;--ink:#10141c;--soft:#566070;--rule:#d9dee6;--wash:#eef1f5;--accent:#1546e0;"
       "--partial:#e8770c;--good:#4f7f16;--bad:#c4261d;" + _LIGHT_CAT + "}\n"
       "@media (prefers-color-scheme:dark){:root:not([data-theme=light]){" + _DARK_TOKENS + "}}\n"
       ":root[data-theme=dark]{color-scheme:dark;" + _DARK_TOKENS + "}\n:root[data-theme=light]{color-scheme:light}\n") + """
*{box-sizing:border-box}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--page);color:var(--ink);font:400 15px/1.45 Roboto,system-ui,-apple-system,"Segoe UI",sans-serif;font-variant-numeric:tabular-nums}
.cond,h1,h2,.hero-n,.stat-v,.aname,th{font-family:"Roboto Condensed","Arial Narrow",Roboto,system-ui,sans-serif}
main{max-width:1280px;margin:0 auto;padding:0 16px 48px}
a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}
a:focus-visible,button:focus-visible,input:focus-visible,summary:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.bar{display:flex;flex-wrap:wrap;justify-content:space-between;align-items:center;gap:8px 24px;padding:16px 0;border-bottom:2px solid var(--ink)}
h1{font-size:20px;line-height:24px;margin:0;font-weight:700;text-transform:uppercase;letter-spacing:.04em}
h1 span{color:var(--soft)}
.live{display:flex;align-items:center;gap:16px;font-size:13px;line-height:20px;color:var(--soft)}
.status{display:flex;align-items:center;gap:8px}
.dot-live{width:8px;height:8px;background:var(--good);animation:pulse 2s ease-in-out infinite}
@keyframes pulse{50%{opacity:.25}}@media (prefers-reduced-motion:reduce){.dot-live{animation:none}}
.tbtn{font:500 13px/20px Roboto,system-ui,sans-serif;color:var(--ink);background:none;border:1px solid var(--rule);padding:4px 12px;cursor:pointer;min-width:72px}
.tbtn:hover{border-color:var(--ink)}
.hero{display:flex;flex-wrap:wrap;align-items:baseline;gap:8px 24px;padding:32px 0 24px}
.hero-n{font-size:72px;line-height:72px;font-weight:700;letter-spacing:-.02em}
.hero-t{display:flex;flex-direction:column;gap:4px}.hero-l{font:700 16px/24px "Roboto Condensed",Roboto,sans-serif;text-transform:uppercase;letter-spacing:.04em}
.note{color:var(--soft);margin:0;max-width:640px}.note b{color:var(--ink);font-weight:500}
.delta{display:block;white-space:nowrap;font-size:13px;line-height:20px;color:var(--soft)}
.strip{display:grid;grid-template-columns:repeat(auto-fit,minmax(144px,1fr));border-top:1px solid var(--rule);border-left:1px solid var(--rule)}
.stat{padding:16px;border-right:1px solid var(--rule);border-bottom:1px solid var(--rule)}
.stat-l{font-size:13px;line-height:20px;color:var(--soft)}.stat-v{font-size:20px;line-height:28px;font-weight:700}
.stat-h{font-size:13px;line-height:20px;color:var(--soft)}
.cols{display:block}
.sec{margin-top:40px}
.sh{display:flex;justify-content:space-between;align-items:baseline;gap:8px 16px;flex-wrap:wrap;padding-bottom:8px;border-bottom:1px solid var(--rule)}
h2{font-size:16px;line-height:24px;margin:0;font-weight:700;text-transform:uppercase;letter-spacing:.04em}
.meta,.muted,.small{color:var(--soft)}.meta,.small{font-size:13px;line-height:20px}
.sec-note{color:var(--soft);font-size:13px;line-height:20px;margin:8px 0 16px}
.chart-block{padding-top:16px}.chart-block+.chart-block{border-top:1px solid var(--rule);margin-top:16px}
.chart-block h3{font-size:15px;line-height:24px;margin:0 0 4px;font-weight:500}
.chart{display:block;width:100%;height:auto;margin-top:8px}
.chart .grid{stroke:var(--rule);stroke-width:1}.chart .tick{fill:var(--soft);font-size:12px;font-family:Roboto,system-ui,sans-serif}
.chart .line{fill:none;stroke-width:2;stroke-linejoin:round}
.line.s-open{stroke:var(--accent)}.dot.s-open{fill:var(--accent)}.line.s-unrev{stroke:var(--partial)}.dot.s-unrev{fill:var(--partial)}
.chart .dot{stroke:none}.b-new{fill:var(--accent)}.b-closed{fill:var(--good)}
.legend{display:flex;flex-wrap:wrap;gap:4px 16px;list-style:none;margin:0;padding:0;font-size:13px;line-height:20px;color:var(--soft)}
.legend li{display:flex;align-items:center;gap:8px}.sw{display:inline-block;width:10px;height:10px}
.sw.s-open,.sw.b-new,.sw.k-rev{background:var(--accent)}.sw.s-unrev,.sw.k-wait{background:var(--partial)}.sw.b-closed,.sw.k-done{background:var(--good)}.sw.k-draft{background:var(--soft)}
.empty{margin:16px 0 0}
.arow{display:grid;grid-template-columns:152px 1fr 72px;gap:8px 16px;align-items:center;padding:12px 0;border-bottom:1px solid var(--rule)}
.aname{font-weight:700;font-size:15px;display:flex;align-items:center;gap:8px;min-width:0}
.aname i{width:10px;height:10px;flex:none;background:var(--ac)}.aname small{font:400 13px Roboto,sans-serif;color:var(--soft);margin-left:auto}
.abar{display:flex;height:8px;background:var(--wash);min-width:8px}.abar i{display:block;min-width:2px}
.abar .draft{background:var(--soft)}.abar .wait{background:var(--partial)}.abar .rev{background:var(--accent)}.abar .done{background:var(--good)}
.abar-wrap{display:flex;flex-direction:column;gap:4px;min-width:0}.acount{font-size:13px;line-height:20px;color:var(--soft)}
.spark{display:block;justify-self:end}.spline{fill:none;stroke:var(--soft);stroke-width:1.5;stroke-linejoin:round}
.cluster{margin-top:16px;padding:0 0 16px 16px;border-left:4px solid var(--ac);border-bottom:1px solid var(--rule)}
.cluster:first-of-type{margin-top:16px}
.chead{display:flex;flex-wrap:wrap;justify-content:space-between;gap:4px 16px;align-items:baseline}
h3{font-size:15px;line-height:24px;margin:0;font-weight:700}
.members{list-style:none;margin:8px 0;padding:0}
.members li{display:flex;flex-wrap:wrap;gap:4px 8px;align-items:center;padding:8px 0;border-top:1px solid var(--rule)}
.members li:first-child{border-top:0}.mt{flex:1 1 240px;min-width:0;overflow-wrap:anywhere}
.chips{display:flex;flex-wrap:wrap;gap:8px;margin:8px 0}
.chip,.fbtn{font:500 13px/20px Roboto,system-ui,sans-serif;padding:4px 12px;border:1px solid var(--rule);background:var(--page);color:var(--ink);cursor:pointer}
.chip:hover,.fbtn:hover{border-color:var(--ink)}
.chip.file{font:12px/20px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;padding:0 8px;background:var(--wash);border-color:var(--wash);cursor:default;overflow-wrap:anywhere}
button.chip[aria-pressed=true],.fbtn[aria-pressed=true]{background:var(--ink);border-color:var(--ink);color:var(--page)}
.pill{display:inline-block;font-size:12px;line-height:18px;padding:0 8px;border:1px solid var(--rule);white-space:nowrap;font-weight:500;color:var(--soft)}
.pill.ok{color:var(--good);border-color:var(--good)}.pill.bad{color:var(--bad);border-color:var(--bad)}
.pill.warn{color:var(--ink);border-color:var(--partial);background:var(--wash)}.pill.accent{color:var(--accent);border-color:var(--accent)}
.tw{overflow-x:auto}
table{border-collapse:collapse;width:100%;min-width:560px}
th,td{text-align:left;padding:8px 16px 8px 0;border-bottom:1px solid var(--rule);vertical-align:top}
th{font-size:13px;line-height:20px;color:var(--soft);font-weight:700;text-transform:uppercase;letter-spacing:.04em}
td.n,th.n{text-align:right;white-space:nowrap}th.n,td.n{padding-right:0;padding-left:16px}
.add{color:var(--good)}.del{color:var(--bad)}
.heat{display:block;height:8px;background:var(--wash);min-width:96px}.heat i{display:block;height:100%;background:var(--partial)}
.pls{display:flex;flex-wrap:wrap;gap:4px 8px}
.filters{display:none;gap:16px;flex-wrap:wrap;align-items:center;margin:16px 0}
.js .filters{display:flex}
.filters input{font:inherit;padding:8px 12px;border:1px solid var(--rule);background:var(--page);color:var(--ink);min-width:260px;flex:1 1 260px;max-width:420px;border-radius:0}
.frow{display:flex;flex-wrap:wrap;gap:8px}
.result{font-size:13px;color:var(--soft)}
details.area{border-bottom:1px solid var(--rule)}
details.area>summary{cursor:pointer;padding:12px 0;list-style:none;display:flex;gap:8px;align-items:center;font:700 15px/24px "Roboto Condensed",Roboto,sans-serif}
details.area>summary::-webkit-details-marker{display:none}
details.area>summary::before{content:"";width:10px;height:10px;background:var(--ac);flex:none}
details.area .cnt{font:400 13px Roboto,sans-serif;color:var(--soft);margin-left:auto}
.rt{display:flex;gap:8px;align-items:flex-start}.av{width:20px;height:20px;background:var(--wash);color:transparent;overflow:hidden;flex:none;margin-top:2px}
.vh{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
tr[hidden],details[hidden]{display:none}
footer{margin-top:48px;padding-top:16px;border-top:1px solid var(--rule);color:var(--soft);font-size:13px;line-height:20px}
@media (min-width:1100px){.cols{display:grid;grid-template-columns:1fr 1fr;gap:0 48px;align-items:start}}
@media (max-width:640px){.arow{grid-template-columns:1fr auto}.abar-wrap{grid-column:1/-1;order:3}.hero-n{font-size:56px;line-height:56px}}
"""

THEME_JS = """
(function(){try{var t=localStorage.getItem('prscan-theme');if(t==='light'||t==='dark')document.documentElement.setAttribute('data-theme',t);}catch(e){}})();
"""

JS = """
(function(){
var d=document,root=d.documentElement;root.classList.add('js');
var tb=d.getElementById('theme');
function th(){return root.getAttribute('data-theme')||'auto'}
function lab(){if(tb)tb.textContent='Theme: '+th()}
if(tb){lab();tb.addEventListener('click',function(){var n={auto:'light',light:'dark',dark:'auto'}[th()];
if(n==='auto')root.removeAttribute('data-theme');else root.setAttribute('data-theme',n);
try{if(n==='auto')localStorage.removeItem('prscan-theme');else localStorage.setItem('prscan-theme',n);}catch(e){}lab();});}
var u=d.getElementById('upd'),a=d.getElementById('ago');
if(u&&a){var m=Math.round((Date.now()-Date.parse(u.getAttribute('datetime')))/6e4);
a.textContent='Updated '+(m<1?'just now':m<60?m+' min ago':m<2880?Math.round(m/60)+' h ago':Math.round(m/1440)+' days ago')+' \\u00b7 ';}
var F=d.getElementById('filters');if(!F)return;
var rows=[].slice.call(d.querySelectorAll('tr.pr')),groups=[].slice.call(d.querySelectorAll('details.area')),
q=d.getElementById('q'),res=d.getElementById('result'),btns=[].slice.call(F.querySelectorAll('button'));
var st={q:'',area:'',draft:0,unrev:0,done:0};
function dec(s){try{return decodeURIComponent(s.replace(/\\+/g,' '))}catch(e){return ''}}
function parse(){st={q:'',area:'',draft:0,unrev:0,done:0};
(location.hash||'').replace(/^#/,'').split('&').forEach(function(kv){var i=kv.indexOf('=');if(i<1)return;
var k=kv.slice(0,i),v=dec(kv.slice(i+1));if(k==='q'||k==='area')st[k]=v;else if(k in st)st[k]=v==='1'?1:0;});}
function write(){var p=[];if(st.q)p.push('q='+encodeURIComponent(st.q));if(st.area)p.push('area='+encodeURIComponent(st.area));
['draft','unrev','done'].forEach(function(k){if(st[k])p.push(k+'=1')});
history.replaceState(null,'',p.length?'#'+p.join('&'):location.pathname+location.search);}
function apply(){var t=st.q.toLowerCase().trim(),on=!!(t||st.area||st.draft||st.unrev||st.done),shown=0;
rows.forEach(function(r){var ok=(!t||r.getAttribute('data-q').indexOf(t)>=0)&&(!st.area||r.getAttribute('data-area')===st.area)&&
(!st.draft||r.getAttribute('data-draft')==='1')&&(!st.unrev||r.getAttribute('data-unrev')==='1')&&(!st.done||r.getAttribute('data-done')==='1');
r.hidden=!ok;if(ok)shown++;});
groups.forEach(function(g){var n=g.querySelectorAll('tr.pr:not([hidden])').length,c=g.querySelector('.cnt'),tot=g.querySelectorAll('tr.pr').length;
g.hidden=n===0;if(on)g.open=n>0;if(c)c.textContent=on?n+' of '+tot:String(tot);});
btns.forEach(function(b){var k=b.getAttribute('data-flag'),ar=b.getAttribute('data-area');
b.setAttribute('aria-pressed',String(k?!!st[k]:ar===st.area));});
res.textContent=on?'Showing '+shown+' of '+rows.length+' pull requests':rows.length+' pull requests';}
function sync(){q.value=st.q;apply();}
q.addEventListener('input',function(){st.q=q.value;write();apply();});
F.addEventListener('click',function(e){var b=e.target.closest('button');if(!b)return;var k=b.getAttribute('data-flag'),ar=b.getAttribute('data-area');
if(k)st[k]=st[k]?0:1;else st.area=st.area===ar?'':ar;write();apply();});
window.addEventListener('hashchange',function(){parse();sync();var el=d.getElementById('all');if(el&&st.area)el.scrollIntoView();});
parse();sync();
})();
"""


def sec_head(title, meta="", sid=""):
    return '<div class="sh"%s><h2>%s</h2><span class="meta">%s</span></div>' % (' id="%s"' % sid if sid else "", esc(title), esc(meta))


def cluster_label(members):
    """(heading, area) for a cluster: the most common primary area when it covers more than half of the
    members, else "N PRs across A, B and C" (top 3). Ties: more changed lines, then alphabetical.
    The area is the top-ranked one, used for the colour bar. members: enriched PR dicts."""
    count, lines = {}, {}
    for p in members:
        a = p["primary"]
        count[a] = count.get(a, 0) + 1
        lines[a] = lines.get(a, 0) + p.get("additions", 0) + p.get("deletions", 0)
    ranked = sorted(count, key=lambda a: (-count[a], -lines[a], a))
    n = len(members)
    if count[ranked[0]] * 2 > n:
        return "%d PRs, mostly %s" % (n, ranked[0]), ranked[0]
    top = ranked[:3]
    names = top[0] if len(top) == 1 else ", ".join(top[:-1]) + " and " + top[-1]
    return "%d PRs across %s" % (n, names), ranked[0]


def cat_style(area):
    i = ALL_AREAS.index(area) if area in ALL_AREAS else len(ALL_AREAS) + sum(map(ord, area))
    return "--ac:var(--cat%d)" % (i % len(CATEGORY))


def render_html(an, repo, now, hist=None):
    hist = hist or []
    prs = an["prs"]
    by_num = {p["number"]: p for p in prs}
    hot_set = {h["file"] for h in an["hotFiles"]}
    areas = sorted(an["areas"], key=lambda a: (-len(an["areas"][a]), a))

    def plink(n):
        return '<a href="%s">#%d</a>' % (esc(safe_url(by_num[n]["url"])), n)

    def vals(key, cur):
        return cur, change_24h(hist, key, cur, now)

    n_un, n_dr = sum(p["unreviewed"] for p in prs), sum(bool(p["isDraft"]) for p in prs)
    n_done, n_stale = sum(p["looksDone"] for p in prs), sum(p["stale"] for p in prs)
    o = []
    o.append("<!doctype html><html lang=en><head><meta charset=utf-8>"
             "<meta name=viewport content='width=device-width,initial-scale=1'>"
             "<title>PR scan: %s</title>"
             "<link rel=preconnect href='https://fonts.googleapis.com'><link rel=preconnect href='https://fonts.gstatic.com' crossorigin>"
             "<link rel=stylesheet href='%s'><script>%s</script><style>%s</style></head><body><main>" % (
                 esc(repo), FONTS_URL, THEME_JS, CSS))
    o.append('<header class="bar"><h1>PR scan <span>/ <a href="https://github.com/%s">%s</a></span></h1>'
             '<div class="live"><span class="status"><i class="dot-live" aria-hidden="true"></i>'
             '<span><span id="ago">Updated </span><time id="upd" datetime="%s">%s UTC</time></span></span>'
             '<button type="button" class="tbtn" id="theme" aria-label="Colour theme">Theme</button></div></header>' % (
                 esc(repo), esc(repo), now.strftime(TS_FMT), now.strftime("%Y-%m-%d %H:%M")))

    d_open = change_24h(hist, "open", len(prs), now)
    o.append('<section class="hero" aria-label="Summary"><div class="hero-n">%d</div><div class="hero-t">'
             '<div class="hero-l">Open pull requests</div>%s'
             '<p class="note"><b>%d</b> unreviewed &middot; <b>%d</b> look done &middot; <b>%d</b> possible duplicate group%s</p></div></section>' % (
                 len(prs), delta_text(d_open), n_un, n_done, len(an["clusters"]), "" if len(an["clusters"]) == 1 else "s"))
    o.append('<section class="strip" aria-label="Stats">%s</section>' % "".join((
        tile("Unreviewed", *vals("unreviewed", n_un)),
        tile("Looks done", *vals("looksDone", n_done)),
        tile("Possible duplicates", *vals("clusters", len(an["clusters"]))),
        tile("Drafts", *vals("draft", n_dr)),
        tile("Stale (%dd+)" % STALE_DAYS, n_stale))))

    # left column: duplicates, trends
    left = []
    left.append('<section class="sec">%s' % sec_head("Possible duplicate work", "%d group%s" % (len(an["clusters"]), "" if len(an["clusters"]) == 1 else "s")))
    if not an["clusters"]:
        left.append('<p class="sec-note">None.</p>')
    else:
        left.append('<p class="sec-note">Linked by changed files (Jaccard &ge; %.1f) or title words (Jaccard &ge; %.1f). Hot files are ignored.</p>' % (FILE_JACCARD, TITLE_JACCARD))
        for c in an["clusters"]:
            count = {}
            for n in c["members"]:
                for path in {f["path"] for f in by_num[n].get("files", [])}:
                    count[path] = count.get(path, 0) + 1
            shared = sorted(p for p, k in count.items() if k >= 2 and p not in hot_set)
            chips = "".join('<span class="chip file">%s</span>' % esc(p) for p in shared[:8])
            if len(shared) > 8:
                chips += '<span class="chip file">+%d more</span>' % (len(shared) - 8)
            members = "".join(
                '<li>%s<span class="mt"><a href="%s">#%d</a> %s <span class="small">by %s</span></span>%s</li>' % (
                    avatar(by_num[n]["author"]), esc(safe_url(by_num[n]["url"])), n, esc(by_num[n]["title"]),
                    esc(by_num[n]["author"]), state_pills(by_num[n])) for n in c["members"])
            label, label_area = cluster_label([by_num[n] for n in c["members"]])
            left.append('<div class="cluster" style="%s"><div class="chead"><h3>%s</h3><span class="small">%s</span></div>'
                        '<ul class="members">%s</ul>%s<p class="small" style="margin:0">Why: %s</p></div>' % (
                            cat_style(label_area), esc(label),
                            esc(", ".join("#%d" % n for n in c["members"])),
                            members, '<div class="chips">%s</div>' % chips if chips else "", esc(cluster_why(c))))
    left.append("</section>")

    recent = window(hist, now, TREND_DAYS)
    if len(recent) >= 2:
        days = daily_summary(recent)
        left.append('<section class="sec">%s' % sec_head("Trends", "last %d days, %d snapshots" % (TREND_DAYS, len(recent))))
        left.append('<div class="chart-block"><h3>Open and unreviewed</h3>%s%s</div>' % (
            legend([("Open", "s-open"), ("Unreviewed", "s-unrev")]),
            svg_line_chart([
                {"name": "open", "cls": "s-open", "points": [(parse_time(s["ts"]), s["open"]) for s in recent]},
                {"name": "unreviewed", "cls": "s-unrev", "points": [(parse_time(s["ts"]), s["unreviewed"]) for s in recent]}],
                "Open and unreviewed pull requests over the last %d days" % TREND_DAYS)))
        left.append('<div class="chart-block"><h3>Opened and closed per day</h3>%s%s</div></section>' % (
            legend([("Opened", "b-new"), ("Closed or merged", "b-closed")]),
            svg_bars(days, "Pull requests opened and closed per day (UTC)")))

    # right column: areas, ready to merge
    right = []
    right.append('<section class="sec">%s' % sec_head("By area", "line: open count, %d days" % SPARK_DAYS))
    right.append('<div style="padding:12px 0 0">%s</div><div class="arows">' % legend([(n, "k-" + k) for k, n in BUCKETS]))
    spark_hist = window(hist, now, SPARK_DAYS)
    biggest = max([len(ps) for ps in an["areas"].values()] or [1])
    for a in areas:
        ps = an["areas"][a]
        cnt = {k: sum(bucket(p) == k for p in ps) for k, _ in BUCKETS}
        segs = "".join('<i class="%s" style="flex:%d" title="%s"></i>' % (k, cnt[k], esc("%s: %d" % (n, cnt[k])))
                       for k, n in BUCKETS if cnt[k])
        series = downsample([s["areas"].get(a, 0) for s in spark_hist], 48)
        right.append('<div class="arow" style="%s"><div class="aname"><i></i><a href="#area=%s">%s</a><small>%d</small></div>'
                     '<div class="abar-wrap"><div class="abar" style="width:%.1f%%">%s</div>'
                     '<div class="acount">%s</div></div>%s</div>' % (
                         cat_style(a), esc(urllib.parse.quote(slug(a), safe="-")), esc(a), len(ps), 100.0 * len(ps) / biggest, segs,
                         esc(" · ".join("%d %s" % (cnt[k], n.lower()) for k, n in BUCKETS if cnt[k])),
                         sparkline(series, "%s: open PRs over %d days" % (a, SPARK_DAYS))))
    right.append("</div></section>")

    done = sorted([p for p in prs if p["looksDone"]], key=lambda p: (-p["ageDays"], p["number"]))
    right.append('<section class="sec">%s' % sec_head("Ready to merge", "%d, oldest first" % len(done)))
    right.append('<p class="sec-note">Not draft, CI green, and unreviewed or approved.</p>')
    if not done:
        right.append('<p class="sec-note">None.</p>')
    else:
        right.append('<div class="tw"><table><thead><tr><th>PR<th>Area<th class="n">Age<th>CI<th class="n">Size</tr></thead><tbody>%s</tbody></table></div>' % "".join(
            '<tr><td><a href="%s">#%d</a> %s</td><td>%s</td><td class="n">%dd</td><td>%s</td>'
            '<td class="n"><span class="add">+%d</span> <span class="del">&minus;%d</span> &middot; %s files</td></tr>' % (
                esc(safe_url(p["url"])), p["number"], esc(p["title"]), esc(p["primary"]), p["ageDays"], ci_pill(p["ci"]),
                p["additions"], p["deletions"], esc(pr_files_note(p))) for p in done))
    right.append("</section>")
    o.append('<div class="cols"><div>%s</div><div>%s</div></div>' % ("".join(left), "".join(right)))

    o.append('<section class="sec">%s' % sec_head("Hot files", "touched by more than %g open PRs" % an["hotThreshold"]))
    o.append('<p class="sec-note">Conflict hotspots, left out of the duplicate groups.</p>')
    if not an["hotFiles"]:
        o.append('<p class="sec-note">None.</p>')
    else:
        top = max(h["count"] for h in an["hotFiles"])
        o.append('<div class="tw"><table><thead><tr><th>File<th class="n">PRs<th>Heat<th>Open PRs</tr></thead><tbody>%s</tbody></table></div>' % "".join(
            '<tr><td style="overflow-wrap:anywhere">%s</td><td class="n">%d</td><td><span class="heat"><i style="width:%d%%"></i></span></td>'
            '<td><div class="pls">%s</div></td></tr>' % (
                esc(h["file"]), h["count"], round(100 * h["count"] / top),
                " ".join(plink(n) for n in h["prs"][:12]) + (" +%d more" % (len(h["prs"]) - 12) if len(h["prs"]) > 12 else ""))
            for h in an["hotFiles"]))
    o.append("</section>")

    o.append('<section class="sec">%s' % sec_head("All PRs by area", "%d pull requests" % len(prs), "all"))
    o.append('<div class="filters" id="filters"><label class="vh" for="q">Search pull requests</label>'
             '<input id="q" type="search" placeholder="Search title, number, author or path" autocomplete="off">'
             '<div class="frow" role="group" aria-label="State filters">%s</div>'
             '<div class="frow" role="group" aria-label="Area filters">%s</div>'
             '<div class="result" id="result" role="status"></div></div>' % (
                 "".join('<button type="button" class="fbtn" data-flag="%s" aria-pressed="false">%s</button>' % (k, n)
                         for k, n in (("draft", "Draft"), ("unrev", "Unreviewed"), ("done", "Looks done"))),
                 "".join('<button type="button" class="chip" data-area="%s" aria-pressed="false">%s</button>' % (
                     esc(slug(a)), esc(a)) for a in areas)))
    for a in areas:
        ps = sorted(an["areas"][a], key=lambda p: p["number"])
        rows = []
        for p in ps:
            also = [x for x in p["areas"] if x != p["primary"]]
            q = " ".join([str(p["number"]), "#%d" % p["number"], p["title"], p["author"]] + [f["path"] for f in p.get("files", [])]).lower()
            rows.append(
                '<tr class="pr" data-area="%s" data-draft="%d" data-unrev="%d" data-done="%d" data-q="%s">'
                '<td><div class="rt">%s<a href="%s">#%d</a></div></td>'
                '<td><div>%s%s</div><div class="pls" style="margin-top:4px">%s</div></td>'
                '<td class="n">%dd</td><td class="n"><span class="add">+%d</span> <span class="del">&minus;%d</span></td>'
                '<td class="n">%s</td></tr>' % (
                    esc(slug(a)), bool(p["isDraft"]), p["unreviewed"], p["looksDone"], esc(q),
                    avatar(p["author"]), esc(safe_url(p["url"])), p["number"],
                    esc(p["title"]), ' <span class="small">(also %s)</span>' % esc(", ".join(also)) if also else "",
                    state_pills(p), p["ageDays"], p["additions"], p["deletions"], esc(pr_files_note(p))))
        o.append('<details class="area" id="%s" style="%s"><summary>%s <span class="cnt">%d</span></summary><div class="tw"><table>'
                 '<thead><tr><th>PR<th>Title<th class="n">Age<th class="n">Lines<th class="n">Files</tr></thead><tbody>%s</tbody></table></div></details>' % (
                     esc(slug(a)), cat_style(a), esc(a), len(ps), "".join(rows)))
    o.append("</section>")
    o.append("<footer>Generated %s UTC for %s. Read-only scan of public PR metadata.</footer></main><script>%s</script></body></html>" % (
        now.strftime("%Y-%m-%d %H:%M"), esc(repo), JS))
    return "".join(o)


def render_md(an, repo, now, hist=None):
    hist = hist or []
    prs = an["prs"]
    by_num = {p["number"]: p for p in prs}

    def plink(p):
        return "[#%d](%s)" % (p["number"], safe_url(p["url"]))

    def prow(p):
        t = md_esc(p["title"]) + (" (draft)" if p["isDraft"] else "")
        return "| %s | %s | %s | %dd | %s | %s | %s |" % (
            plink(p), t, md_esc(p["author"]), p["ageDays"], md_esc(review_state(p)), md_esc(p["ci"] or "none"), pr_files_note(p))

    def ch(key, cur):
        d = change_24h(hist, key, cur, now)
        return "" if d is None else " (%+d in 24 h)" % d

    head = ["| PR | Title | Author | Age | Review | CI | Files |", "|---|---|---|---|---|---|---|"]
    n_un, n_dr = sum(p["unreviewed"] for p in prs), sum(bool(p["isDraft"]) for p in prs)
    n_done, n_stale = sum(p["looksDone"] for p in prs), sum(p["stale"] for p in prs)
    o = ["# Open pull requests: %s" % repo, "", "Updated %s UTC." % now.strftime("%Y-%m-%d %H:%M"), ""]
    o.append("Open %d%s, unreviewed %d%s, looks done %d%s, possible duplicates (clusters) %d%s, drafts %d%s, stale %d." % (
        len(prs), ch("open", len(prs)), n_un, ch("unreviewed", n_un), n_done, ch("looksDone", n_done),
        len(an["clusters"]), ch("clusters", len(an["clusters"])), n_dr, ch("draft", n_dr), n_stale))
    recent = window(hist, now, TREND_DAYS)
    if len(recent) >= 2:
        o += ["", "## Trends", "", "Last %d days, last snapshot of each UTC day; opened and closed from consecutive snapshots." % TREND_DAYS, "",
              "| Day | Open | Unreviewed | Opened | Closed |", "|---|---|---|---|---|"]
        o += ["| %s | %d | %d | %d | %d |" % (d["day"], d["open"], d["unreviewed"], d["new"], d["closed"]) for d in daily_summary(recent)]
    areas = sorted(an["areas"], key=lambda a: (-len(an["areas"][a]), a))
    o += ["", "## By area", "", "| Area | Open | Draft | Waiting for review | Reviewed | Looks done | + lines | - lines |", "|---|---|---|---|---|---|---|---|"]
    for a in areas:
        ps = an["areas"][a]
        c = [sum(bucket(p) == k for p in ps) for k, _ in BUCKETS]
        o.append("| [%s](#%s) | %d | %d | %d | %d | %d | %d | %d |" % (
            md_esc(a), slug(a), len(ps), c[0], c[1], c[2], c[3], sum(p["additions"] for p in ps), sum(p["deletions"] for p in ps)))
    o += ["", "## Possible duplicate work", ""]
    if not an["clusters"]:
        o += ["None.", ""]
    for c in an["clusters"]:
        o += ["- " + ", ".join(plink(by_num[n]) for n in c["members"]) + " : " + md_esc(cluster_why(c))]
        o += ["  - %s %s (%s)" % (plink(by_num[n]), md_esc(by_num[n]["title"]), md_esc(by_num[n]["primary"])) for n in c["members"]]
    o += ["", "## Ready to merge", ""]
    done = sorted([p for p in prs if p["looksDone"]], key=lambda p: (-p["ageDays"], p["number"]))
    o += (head + [prow(p) for p in done]) if done else ["None."]
    o += ["", "## Hot files", "", "Files touched by more than %g open PRs: conflict hotspots. They are left out of the duplicate clusters." % an["hotThreshold"], ""]
    if an["hotFiles"]:
        o += ["| File | Open PRs | PRs |", "|---|---|---|"]
        o += ["| %s | %d | %s |" % (md_esc(h["file"]), h["count"], ", ".join(plink(by_num[n]) for n in h["prs"])) for h in an["hotFiles"]]
    else:
        o.append("None.")
    o += ["", "## All PRs by area", ""]
    for a in areas:
        ps = sorted(an["areas"][a], key=lambda p: p["number"])
        o += ['<a id="%s"></a>' % slug(a), "", "### %s (%d)" % (md_esc(a), len(ps)), ""] + head + [prow(p) for p in ps] + [""]
    stale = [p for p in prs if p["stale"]]
    o += ["## Stale", "", "No update in %d days or more." % STALE_DAYS, ""]
    o += (head + [prow(p) for p in sorted(stale, key=lambda p: p["number"])]) if stale else ["None."]
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
    hist = []
    if args.history:
        hist = prune_history(append_snapshot(read_history(args.history), snapshot(an, now)), now)
        try:
            write(args.history, history_text(hist))
        except OSError as e:
            print("pr-scan: could not write history: %s" % e, file=sys.stderr)
    write(os.path.join(args.out, "index.html"), render_html(an, args.repo, now, hist))
    write(os.path.join(args.out, "report.md"), render_md(an, args.repo, now, hist))
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
    r.add_argument("--history", default=None, metavar="FILE",
                   help="JSON-lines snapshot history: read, append the current snapshot (if the last is 30+ min old), prune to 90 days, render trends")
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
