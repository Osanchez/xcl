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
        d = days.setdefault(day, {"day": day, "open": 0, "unreviewed": 0, "new": 0, "closed": 0, "pairs": 0})
        d["open"], d["unreviewed"] = s["open"], s["unreviewed"]
        if prev is not None and prev["prs"] is not None and s["prs"] is not None:
            n, c = new_and_closed(prev["prs"], s["prs"])
            d["new"] += n
            d["closed"] += c
            d["pairs"] += 1
        prev = s
    return [days[k] for k in sorted(days)]


def change_24h(hist, key, current, now):
    """current - value in the snapshot closest to 24 h ago, among those at least 23 h old; None without one."""
    day = dt.timedelta(hours=24)
    base = [s for s in hist if now - parse_time(s["ts"]) >= dt.timedelta(hours=23)]
    if not base:
        return None
    best = min(base, key=lambda s: abs((now - parse_time(s["ts"])) - day))
    return current - best[key]


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
        return '<span class="delta">no 24 h data yet</span>'
    if d == 0:
        return '<span class="delta">no change / 24 h</span>'
    return '<span class="delta">%s %d / 24 h</span>' % ("&#9650;" if d > 0 else "&#9660;", abs(d))


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
    if max([len(s["points"]) for s in series] or [0]) < 2:
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
        body.append('<text class="tick" x="%.1f" y="%d" text-anchor="%s">%s</text>' % (
            x(t), h - 8, "end" if x(t) > w - 22 else "middle", esc(text)))
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
            tip_text = "%s %s UTC: %s %d" % (_md(t), t.strftime("%H:%M"), s["name"], v)
            body.append('<circle class="dot %s" cx="%.1f" cy="%.1f" r="%d" data-tip="%s"><title>%s</title></circle>' % (
                esc(s["cls"]), x(t), y(v), 4 if len(sp) == 1 else 3,
                esc(tip_text), esc(tip_text)))
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
            body.append('<rect class="%s" x="%.1f" y="%.1f" width="%.1f" height="%.1f" data-tip="%s"><title>%s</title></rect>' % (
                cls, bx, h - mb - bh, bw, max(bh, 0), esc("%s: %d %s" % (r["day"], r[key], name)), esc("%s: %d %s" % (r["day"], r[key], name))))
        if (len(rows) - 1 - i) % every == 0:
            body.append('<text class="tick" x="%.1f" y="%d" text-anchor="middle">%s</text>' % (cx, h - 8, esc(r["day"][5:])))
    return svg_frame(w, h, label, "".join(body))


def sparkline(vals, label="", w=72, h=24):
    cls = 'class="spark" width="%d" height="%d" viewBox="0 0 %d %d" preserveAspectRatio="none"' % (w, h, w, h)
    if len(vals) < 2:
        return '<span class="nodata">no data</span>'
    lo, hi = min(vals), max(vals)
    pts = " ".join("%.1f,%.1f" % (2 + (w - 4) * i / (len(vals) - 1),
                                  h / 2 if hi == lo else h - 3 - (h - 6) * (v - lo) / (hi - lo)) for i, v in enumerate(vals))
    return '<svg %s role="img" aria-label="%s" data-tip="%s"><title>%s</title><polyline class="spline" points="%s"/></svg>' % (
        cls, esc(label), esc(label), esc(label), pts)


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


# Page tokens (light, dark): the fixed sunset gradient, top to bottom, and the translucent surface of panels, top bar
# and callouts. The scene itself has no backdrop: the canvas is transparent and sits on this gradient.
GRADIENT_TOKENS = [("dusk-1", "#ffd8a8", "#3b2a5a"), ("dusk-2", "#ff9f80", "#5a2f55"),
                   ("dusk-3", "#e9849c", "#2a1f3d"), ("dusk-4", "#b39ddb", "#0e1219"),
                   ("surface", "rgba(255,255,255,.78)", "rgba(14,18,25,.74)")]
_LIGHT_SCENE = "".join("--%s:%s;" % (n, a) for n, a, _ in GRADIENT_TOKENS)
_DARK_SCENE = "".join("--%s:%s;" % (n, b) for n, _, b in GRADIENT_TOKENS)
_LIGHT_CAT, _DARK_CAT = category_css()
_DARK_TOKENS = ("--page:#0e1219;--ink:#eef2f8;--soft:#a2adbc;--rule:#2a3240;--wash:#1a212c;--accent:#8ca8ff;--partial:#ffab5c;--partial-text:#ffab5c;"
                "--empty:#39424f;--tip:#eef2f8;--tip-text:#0e1219;--tip-soft:#3a4454;--good:#b4e06a;--bad:#ff8a80;" + _DARK_SCENE + _DARK_CAT)
CSS = (":root{color-scheme:light dark;--page:#fff;--ink:#10141c;--soft:#566070;--rule:#d9dee6;--wash:#eef1f5;--accent:#1546e0;"
       "--partial:#e8770c;--partial-text:#a94f00;--empty:#c9cfd9;--tip:#10141c;--tip-text:#fff;--tip-soft:#d7dce4;--good:#456b12;--bad:#c4261d;"
       + _LIGHT_SCENE + _LIGHT_CAT + "}\n"
       "@media (prefers-color-scheme:dark){:root:not([data-theme=light]){" + _DARK_TOKENS + "}}\n"
       ":root[data-theme=dark]{color-scheme:dark;" + _DARK_TOKENS + "}\n:root[data-theme=light]{color-scheme:light}\n") + """
*{box-sizing:border-box}
html{background:var(--page);-webkit-text-size-adjust:100%}
body::before{content:"";position:fixed;top:0;right:0;bottom:0;left:0;z-index:-1;pointer-events:none;background:linear-gradient(180deg,var(--dusk-1),var(--dusk-2),var(--dusk-3),var(--dusk-4))}
body{margin:0;color:var(--ink);font:400 15px/1.45 Roboto,system-ui,-apple-system,"Segoe UI",Arial,sans-serif;font-variant-numeric:tabular-nums}
h1,h2,h3,p,ol,ul{margin:0;padding:0}ol,ul{list-style:none}
h1,h2,h3,.cond,.score,.stat b,.rank,.step b,th{font-family:"Roboto Condensed","Arial Narrow",Roboto,system-ui,sans-serif}
a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}
a:focus-visible,button:focus-visible,input:focus-visible,summary:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.app{display:grid;grid-template-columns:minmax(0,1fr);padding:0 16px;row-gap:8px;
grid-template-areas:"top" "city" "trends" "road" "standings" "feed" "meta" "foot"}
.app>*{min-width:0}
.top{grid-area:top;display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:6px 14px;min-height:50px;padding:6px 12px;background:var(--surface);border-bottom:2px solid var(--ink)}
h1{font:700 20px/1.1 "Roboto Condensed",Arial,sans-serif;text-transform:uppercase;letter-spacing:.04em}h1 span{color:var(--soft)}
h2{font-size:16px;line-height:1.2;font-weight:700}h3{font-size:15px;line-height:1.3;font-weight:700}
.top-right{display:flex;align-items:center;gap:12px;min-width:0}
.live{display:flex;align-items:center;gap:8px;color:var(--soft);font-size:13px;white-space:nowrap}
.dot{width:8px;height:8px;background:var(--accent);animation:beat 2s steps(2) infinite}
@keyframes beat{50%{opacity:.25}}
.theme{border:1px solid var(--rule);background:transparent;color:var(--ink);font:500 13px/1 Roboto,Arial,sans-serif;padding:7px 10px;cursor:pointer}
.theme:hover{border-color:var(--ink)}
.toolbar{display:none;order:3;flex:1 1 100%;align-items:center;flex-wrap:wrap;gap:6px 8px;min-width:0}
.js .toolbar{display:flex}
.toolbar input,.toolbar select{font:500 13px/1.2 Roboto,Arial,sans-serif;padding:7px 10px;border:1px solid var(--rule);background:var(--page);color:var(--ink);min-width:0;border-radius:0}
.toolbar input{flex:1 1 140px;max-width:300px}.toolbar select{flex:0 1 190px;max-width:100%;text-overflow:ellipsis}
.seg{display:inline-flex;max-width:100%;border:1px solid var(--rule)}
.seg button{flex:none;border:0;border-right:1px solid var(--rule);background:transparent;color:var(--ink);font:500 13px/1 Roboto,Arial,sans-serif;padding:7px 10px;cursor:pointer;border-radius:0}
.seg button:last-child{border-right:0}.seg button:hover{background:var(--wash)}
.seg button[aria-pressed=true]{background:var(--ink);color:var(--page)}
.result{font-size:13px;color:var(--soft);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0}.result:empty{display:none}
.a-city,.a-trends,.a-road,.a-standings,.a-feed{min-width:0;min-height:0;padding:12px}
.a-trends,.a-road,.a-standings,.a-feed,.a-meta,.foot{background:var(--surface)}
.a-city .score-box,.scene-foot,.scene-key,.a-city>.empty-block,.a-city>.sec-note{background:var(--surface);padding-left:10px;padding-right:10px}
.a-city{grid-area:city}.a-trends{grid-area:trends;display:flex;flex-direction:column}.a-road{grid-area:road;display:flex;flex-direction:column}.a-standings{grid-area:standings;display:flex;flex-direction:column}
.a-feed{grid-area:feed;display:flex;flex-direction:column}
.head{display:flex;justify-content:space-between;align-items:baseline;gap:10px;margin-bottom:8px;min-width:0}
.head>span{color:var(--soft);font-size:13px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.score-box{display:flex;align-items:flex-end;flex-wrap:wrap;gap:8px 16px;margin-bottom:8px}
.score{font:700 72px/.86 "Roboto Condensed",Arial,sans-serif}.score small{font-size:28px;margin-left:6px;color:var(--soft)}
.score-note{color:var(--soft);font-size:13px;line-height:1.35}.score-note b{display:block;color:var(--ink);font-size:15px;font-weight:500}
.score-note span{display:block}
.sec-note{color:var(--soft);font-size:13px;line-height:1.35;margin:4px 0 8px}
.charts{display:grid;grid-template-columns:minmax(0,1fr);gap:12px 16px;flex:1;min-height:0}
.chart-block{display:flex;flex-direction:column;min-width:0;min-height:0}
.chart-head{display:flex;flex-wrap:wrap;align-items:baseline;gap:0 12px}.chart-block h3{font-weight:500}
.chart-box{position:relative;flex:1;min-height:0}
.chart{display:block;width:100%;height:auto;margin-top:4px}
.chart .grid{stroke:var(--rule);stroke-width:1}.chart .tick{fill:var(--soft);font-size:13px;font-family:Roboto,Arial,sans-serif}
.chart .line{fill:none;stroke-width:2;stroke-linejoin:round}
.line.s-open{stroke:var(--accent)}.dot.s-open{fill:var(--accent)}.line.s-unrev{stroke:var(--partial)}.dot.s-unrev{fill:var(--partial)}
.chart .dot{stroke:none}.b-new{fill:var(--accent)}.b-closed{fill:var(--good)}
.legend{display:flex;flex-wrap:wrap;gap:2px 14px;font-size:13px;line-height:1.3;color:var(--soft)}
.legend li{display:flex;align-items:center;gap:6px}.sw{display:inline-block;width:8px;height:8px}
.sw.s-open,.sw.b-new,.sw.k-rev{background:var(--accent)}.sw.s-unrev,.sw.k-wait{background:var(--partial)}.sw.b-closed,.sw.k-done{background:var(--good)}.sw.k-draft{background:var(--soft)}
.empty{margin:8px 0 0}
.empty-block{margin:8px 0 4px;padding:20px 0;border-top:1px solid var(--rule);border-bottom:1px solid var(--rule);color:var(--soft);font:700 16px/1.2 "Roboto Condensed",Arial,sans-serif;text-transform:uppercase;letter-spacing:.04em}
.road{display:grid;grid-template-columns:repeat(5,minmax(max-content,1fr));gap:3px;flex:1;min-height:92px}
.step{display:flex;flex-direction:column;justify-content:flex-end;gap:4px;padding:8px 4px;background:var(--wash);border-bottom:3px solid var(--empty);font-size:13px;line-height:1.2;min-width:0}
.step b{font-size:22px;line-height:1}.step>span{font-size:12px;font-weight:500;white-space:nowrap}.step small{color:var(--soft);font-size:13px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.step.on{border-bottom-color:var(--sc)}
.pips{display:flex;gap:2px}.pips i{display:block;flex:1;max-width:9px;height:7px;background:var(--empty)}.pips i.on{background:var(--sc)}
.st-axis,.st{display:grid;grid-template-columns:22px minmax(128px,1fr) minmax(40px,72px) 68px;gap:0 8px;align-items:center}
.st-axis{position:sticky;top:0;z-index:1;padding:0 4px 4px 2px;background:var(--page);color:var(--soft);font-size:13px;line-height:1.2}
.st-axis span:last-child{text-align:right}
.st-list{overflow-y:auto;min-height:0;flex:1;max-height:420px}
.st-list,.feed-body{scrollbar-width:thin;scrollbar-color:var(--rule) transparent}
.st-list::-webkit-scrollbar,.feed-body::-webkit-scrollbar{width:8px}
.st-list::-webkit-scrollbar-thumb,.feed-body::-webkit-scrollbar-thumb{background:var(--rule);border-radius:4px}
.st-list::-webkit-scrollbar-thumb:hover,.feed-body::-webkit-scrollbar-thumb:hover{background:var(--soft)}
.st{padding:6px 4px 6px 2px;border-top:1px solid var(--rule);font-size:13px;line-height:1.3;min-height:44px}
.st:hover{background:color-mix(in srgb,var(--ink) 6%,transparent)}
.rank{font-size:16px;font-weight:700;text-align:center;line-height:1}
.who{min-width:0}.who b{display:block;font-size:14px;font-weight:700;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.who small{display:block;color:var(--soft);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.ptype{display:inline-flex;align-items:center;gap:6px;color:var(--ink);font-weight:500;min-width:0}
.ptype{max-width:100%}.ptype .nm{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0}.ptype i{display:block;flex:none;width:8px;height:8px;background:var(--ac)}
.track{display:block;height:12px;background:var(--wash)}.fill{display:flex;height:100%}.fill i{display:block;min-width:2px}
.fill .draft{background:var(--soft)}.fill .wait{background:var(--partial)}.fill .rev{background:var(--accent)}.fill .done{background:var(--good)}
.where{display:grid;justify-items:end;gap:2px;color:var(--soft)}.where b{font:700 16px/1 "Roboto Condensed",Arial,sans-serif;color:var(--ink)}
.spark{display:block;width:100%;height:20px}.spline{vector-effect:non-scaling-stroke;fill:none;stroke:var(--soft);stroke-width:1.5;stroke-linejoin:round}.nodata{font-size:12px;color:var(--soft);white-space:nowrap}
.tabs{display:none;gap:14px}.js .tabs{display:flex}
.tabs button{padding:0 0 2px;border:0;border-bottom:2px solid transparent;background:none;color:var(--soft);font:700 16px/1.2 "Roboto Condensed",Arial,sans-serif;cursor:pointer;border-radius:0}
.tabs button:hover{color:var(--ink)}.tabs button[aria-pressed=true]{border-bottom-color:var(--partial);color:var(--ink)}
.feed-body{overflow:auto;min-height:0;flex:1;max-height:640px}
.panel+.panel{margin-top:24px}.js .panel+.panel{margin-top:0}
.ptitle{display:flex;justify-content:space-between;gap:8px;align-items:baseline;padding-bottom:6px}.ptitle span{color:var(--soft);font-size:13px}.js .ptitle{display:none}
.panel[hidden]{display:none}
.row{display:grid;grid-template-columns:30px minmax(0,1fr);gap:8px;padding:7px 4px 7px 2px;border-top:1px solid var(--rule);font-size:13px;line-height:1.35}
.row:hover{background:color-mix(in srgb,var(--ink) 6%,transparent)}
.row .lead{color:var(--soft)}.row .lead b{display:block;color:var(--ink);font:700 15px/1.2 "Roboto Condensed",Arial,sans-serif}
.row .ttl{overflow-wrap:anywhere;font-size:14px}.row .ttl a{font-weight:500}
.sys{display:flex;flex-wrap:wrap;align-items:center;gap:2px 10px;color:var(--soft);font-size:13px;margin-top:2px}
.av{width:20px;height:20px;background:var(--wash);color:transparent;overflow:hidden;flex:none}
.pill{display:inline-flex;align-items:center;gap:4px;font-size:13px;line-height:1.3;white-space:nowrap;font-weight:500;color:var(--soft)}
.pill:before{content:"";width:7px;height:7px;background:currentColor;flex:none}
.pill.ok{color:var(--good)}.pill.bad{color:var(--bad)}.pill.warn{color:var(--partial-text)}.pill.accent{color:var(--accent)}
.add{color:var(--good)}.del{color:var(--bad)}
.dup{padding:0 0 12px 12px;margin-bottom:12px;border-left:4px solid var(--ac);border-bottom:1px solid var(--rule)}
.dup .chead{display:flex;flex-wrap:wrap;justify-content:space-between;gap:2px 12px;align-items:baseline}
.dup .row:first-of-type{border-top:0}
.chips-f{display:flex;flex-wrap:wrap;gap:6px;margin:8px 0}
.fchip{font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;padding:0 6px;background:var(--wash);overflow-wrap:anywhere}
.heat{display:block;height:6px;background:var(--wash);margin:4px 0}.heat i{display:block;height:100%;background:var(--partial)}
.mono{font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;overflow-wrap:anywhere}
details.area{border-bottom:1px solid var(--rule)}
details.area>summary{cursor:pointer;padding:8px 2px;list-style:none;display:flex;gap:8px;align-items:center;font:700 15px/1.3 "Roboto Condensed",Arial,sans-serif}
details.area>summary::-webkit-details-marker{display:none}details.area .cnt{margin-left:auto;font:400 13px Roboto,sans-serif;color:var(--soft)}
details.area>summary:before{content:"+";width:12px;color:var(--soft);font-weight:700}details.area[open]>summary:before{content:"\\2212"}
.a-meta{grid-area:meta;display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px 16px;padding:12px}
.stat{min-width:0}.stat span{display:block;color:var(--soft);font-size:13px;line-height:1.25}
.stat b{display:block;font-size:20px;line-height:1.15;white-space:nowrap}.stat.key b{color:var(--accent)}.stat.warn b{color:var(--partial-text)}
.stat small{display:block;color:var(--soft);font-size:13px;line-height:1.25}
.foot{grid-area:foot;padding:8px 12px 10px;color:var(--soft);font-size:13px;line-height:1.35}
.small{color:var(--soft);font-size:13px;line-height:1.35}
.tip{position:fixed;top:0;left:0;z-index:30;max-width:260px;padding:6px 9px;background:var(--tip);color:var(--tip-text);font-size:13px;line-height:1.3;pointer-events:none}
.tip[hidden]{display:none}
.vh{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
[hidden]{display:none!important}
@media (min-width:640px){.a-meta{grid-template-columns:repeat(4,minmax(0,1fr))}}
@media (max-width:760px){.road{grid-template-columns:1fr}.step{display:grid;grid-template-columns:44px minmax(0,1fr) auto;align-items:center;gap:8px;padding:6px 8px;border-bottom-width:2px}.step .pips{display:none}}
@media (max-width:560px){.st{grid-template-columns:22px minmax(0,1fr) 68px}.st .track{grid-column:2/4;grid-row:2}.st-axis{display:none}.score{font-size:56px}}
@media (max-width:620px){.top{flex-wrap:wrap;gap:4px 12px;padding:7px 0}.top-right{width:100%;justify-content:space-between}}
@media (min-width:900px){
:root{--fs:clamp(11px,calc(.42vw + .62vh + 4.2px),14px);--fs-s:max(10.5px,calc(var(--fs) - 1px));--gap:clamp(8px,1.4vw,22px);--pad:clamp(5px,1.1vh,12px)}
html,body{height:100%;overflow:hidden}
body{font-size:var(--fs)}
.app{height:100vh;height:100dvh;overflow:hidden;padding:0 var(--gap);column-gap:var(--gap);row-gap:clamp(3px,.7vh,8px);
grid-template-columns:minmax(0,1.5fr) minmax(0,1fr) minmax(0,1.1fr);grid-template-rows:auto minmax(0,1.7fr) minmax(0,1fr) auto auto auto;
grid-template-areas:"top top top" "city standings feed" "trends standings feed" "road road road" "meta meta meta" "foot foot foot"}
.top{min-height:clamp(38px,6vh,50px);padding:0 12px;flex-wrap:wrap}
h1{font-size:clamp(15px,1.5vw,20px)}h2{font-size:clamp(13px,calc(var(--fs) + 1px),16px)}h3{font-size:var(--fs)}
.toolbar{order:0;flex:1 1 360px;justify-content:center;flex-wrap:nowrap}
.toolbar input,.toolbar select,.seg button,.theme{font-size:var(--fs-s);padding:clamp(4px,.8vh,7px) clamp(6px,.8vw,10px)}
.toolbar input{flex:1 1 90px;max-width:240px}.toolbar select{flex:0 1 clamp(110px,13vw,190px)}
.live,.result,.sec-note,.score-note,.legend,.step,.step small,.st-axis,.st,.row,.sys,.pill,.stat span,.stat small,.foot,.small,.head>span,.scene-cap,.tabs button{font-size:var(--fs-s)}
.a-city,.a-trends,.a-road,.a-standings,.a-feed{overflow:hidden;padding:var(--pad) 12px}
.a-city{display:flex;flex-direction:column}.a-city>.scene{flex:1;min-height:0}
.head{margin-bottom:clamp(2px,.6vh,8px)}
.score{font-size:clamp(34px,7.4vh,72px)}.score small{font-size:.39em}
.score-note,.score-note b{font-size:var(--fs-s)}.score-note b{font-size:var(--fs)}
.charts{grid-template-columns:repeat(2,minmax(0,1fr))}
.chart-box .chart{position:absolute;left:0;top:0;width:100%;height:100%;margin:0}
.road{flex:none;min-height:0;grid-template-columns:repeat(5,minmax(0,1fr))}
.step{flex-direction:row;flex-wrap:wrap;align-items:baseline;justify-content:flex-start;gap:0 8px;padding:clamp(3px,.7vh,7px) 6px;overflow:hidden}.step .pips{display:none}.step b{font-size:clamp(15px,2.7vh,22px)}
.step>span{font-size:var(--fs-s);white-space:normal}
.st-axis,.st{grid-template-columns:clamp(14px,1.6vw,22px) minmax(60px,1fr) minmax(28px,.7fr) clamp(46px,5vw,68px)}
.st{min-height:clamp(34px,5vh,44px);padding-top:clamp(3px,.6vh,6px);padding-bottom:clamp(3px,.6vh,6px)}
.rank{font-size:clamp(13px,var(--fs),16px)}.who b{font-size:var(--fs)}
.st-list,.feed-body{max-height:none;overflow-y:auto}
.a-feed,.a-standings{display:flex;flex-direction:column}
.row .ttl{font-size:var(--fs)}
.a-meta{display:flex;flex-wrap:wrap;justify-content:space-between;gap:2px var(--gap);padding:var(--pad) 12px}.stat{flex:none}
.stat b{font-size:clamp(14px,2.3vh,20px)}
.foot{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;padding:clamp(3px,.6vh,8px) 12px clamp(3px,.8vh,10px)}
}
@media (prefers-reduced-motion:reduce){*,*:before,*:after{animation:none!important;transition:none!important}}
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
var tip=d.getElementById('tip'),tipped=[].slice.call(d.querySelectorAll('[data-tip]'));
tipped.forEach(function(el){var t=el.querySelector('title');if(t)el.removeChild(t);el.removeAttribute('title');});
function place(e){var x=e.clientX+12,y=e.clientY+14,w=tip.offsetWidth,h=tip.offsetHeight;
if(x+w>innerWidth-8)x=Math.max(8,e.clientX-w-12);if(y+h>innerHeight-8)y=Math.max(8,e.clientY-h-12);
tip.style.left=x+'px';tip.style.top=y+'px';}
d.addEventListener('mouseover',function(e){var el=e.target.closest&&e.target.closest('[data-tip]');
if(!el){tip.hidden=true;return;}tip.textContent=el.getAttribute('data-tip');tip.hidden=false;place(e);});
d.addEventListener('mousemove',function(e){if(!tip.hidden)place(e);});
d.addEventListener('mouseleave',function(){tip.hidden=true;});
var tabs=[].slice.call(d.querySelectorAll('.tabs button')),panels=[].slice.call(d.querySelectorAll('.panel')),
tmeta=d.getElementById('tabmeta'),tab='dup';
function showTab(n){tab=n;tabs.forEach(function(b){b.setAttribute('aria-pressed',String(b.getAttribute('data-tab')===n));});
panels.forEach(function(p){var on=p.getAttribute('data-panel')===n;p.hidden=!on;if(on&&tmeta)tmeta.textContent=p.getAttribute('data-meta')||'';});}
tabs.forEach(function(b){b.addEventListener('click',function(){showTab(b.getAttribute('data-tab'));write();});});
var F=d.getElementById('filters');
var rows=[].slice.call(d.querySelectorAll('.pr')),groups=[].slice.call(d.querySelectorAll('details.area')),
q=d.getElementById('q'),sel=d.getElementById('area'),res=d.getElementById('result'),segs=F?[].slice.call(F.querySelectorAll('.seg button')):[];
var st={q:'',area:'',state:''},STATES=['unrev','done','draft'];
function dec(s){try{return decodeURIComponent(s.replace(/\\+/g,' '))}catch(e){return ''}}
function opt(v){if(!sel||!v)return null;for(var i=0;i<sel.options.length;i++)if(sel.options[i].value===v)return sel.options[i];return null;}
function parse(){st={q:'',area:'',state:''};var tt='';
(location.hash||'').replace(/^#/,'').split('&').forEach(function(kv){var i=kv.indexOf('=');if(i<1)return;
var k=kv.slice(0,i),v=dec(kv.slice(i+1));if(k==='q'||k==='area')st[k]=v;else if(k==='tab')tt=v;
else if(k==='state'){if(STATES.indexOf(v)>=0)st.state=v;}else if(STATES.indexOf(k)>=0&&v==='1'&&!st.state)st.state=k;});
if(sel&&st.area&&!opt(st.area))st.area='';return tt;}
function on(){return !!(st.q.trim()||st.area||st.state);}
function write(){var p=[];if(st.q)p.push('q='+encodeURIComponent(st.q));if(st.area)p.push('area='+encodeURIComponent(st.area));
if(st.state)p.push('state='+st.state);if(tab!=='dup')p.push('tab='+tab);
history.replaceState(null,'',p.length?'#'+p.join('&'):location.pathname+location.search);}
function areaOk(r){if(!st.area)return true;var a=r.getAttribute('data-area'),o=opt(st.area),m=o&&o.getAttribute('data-members');
return m?(','+m+',').indexOf(','+a+',')>=0:a===st.area;}
function apply(){var t=st.q.toLowerCase().trim(),f=on(),shown=0;
rows.forEach(function(r){var ok=(!t||r.getAttribute('data-q').indexOf(t)>=0)&&areaOk(r)&&(!st.state||r.getAttribute('data-'+st.state)==='1');
r.hidden=!ok;if(ok)shown++;});
groups.forEach(function(g){var n=g.querySelectorAll('.pr:not([hidden])').length,c=g.querySelector('.cnt'),tot=g.querySelectorAll('.pr').length;
g.hidden=n===0;if(f)g.open=n>0;if(c)c.textContent=f?n+' of '+tot:String(tot);});
segs.forEach(function(b){b.setAttribute('aria-pressed',String(b.getAttribute('data-state')===st.state));});
if(res)res.textContent=f?shown+' of '+rows.length:'';
var allp=d.querySelector('.panel[data-panel=all]');if(allp&&f)allp.setAttribute('data-meta','Showing '+shown+' of '+rows.length);
else if(allp)allp.setAttribute('data-meta',rows.length+' pull requests');
showTab(tab);}
function sync(){if(q)q.value=st.q;if(sel)sel.value=st.area;apply();}
function changed(){if(on())tab='all';write();apply();}
if(F){q.addEventListener('input',function(){st.q=q.value;changed();});
if(sel)sel.addEventListener('change',function(){st.area=sel.value;changed();});
segs.forEach(function(b){b.addEventListener('click',function(){st.state=b.getAttribute('data-state')||'';changed();});});}
window.addEventListener('hashchange',function(){var tt=parse();tab=tt||(on()?'all':tab);sync();var el=d.getElementById('feed');if(el&&on())el.scrollIntoView();});
var t0=parse();tab=t0||(on()?'all':'dup');sync();
})();
"""


def stat(label, value, delta=None, show_delta=True, cls=""):
    return '<div class="stat%s"><span>%s</span><b>%s</b>%s</div>' % (
        " " + cls if cls else "", esc(label), esc(value), delta_text(delta) if show_delta else "")


def ptype(area, link=False):
    inner = '<i></i><span class="nm">%s</span>' % esc(area)
    if link:
        inner = '<a class="ptype" title="%s" href="#area=%s">%s</a>' % (esc(area), esc(urllib.parse.quote(slug(area), safe="-")), inner)
        return '<span style="%s">%s</span>' % (cat_style(area), inner)
    return '<span class="ptype" style="%s">%s</span>' % (cat_style(area), inner)


def road_step(label, count, share, color, note=None):
    """count None means no data; share is 0..1 or None (no pips)."""
    if count is None:
        return '<li class="step" style="--sc:%s"><b>&ndash;</b><span>%s</span><small>no data yet</small></li>' % (color, esc(label))
    pips = ""
    if share is not None:
        n = round(share * 10)
        pips = '<div class="pips" aria-hidden="true">%s</div>' % "".join('<i class="on"></i>' if i < n else "<i></i>" for i in range(10))
    small = note if note is not None else "%d%%" % round(100 * (share or 0))
    return '<li class="step%s" style="--sc:%s"><b>%d</b><span>%s</span>%s%s</li>' % (
        " on" if count else "", color, count, esc(label), pips, "<small>%s</small>" % esc(small) if small else "")


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


# ---- top areas and the effort map

TOP_AREAS = 6
ELSE_NAME = "Everything else"
SCENE_CAP = 40          # tallest tower in layers; above it the layers are scaled
SCENE_W, SCENE_H = 720, 520   # viewBox of the static SVG
SCENE_YAW = -22.0       # degrees, fixed angle of the SVG and centre of the canvas pan
SCENE_ARC = 90.0        # the canvas pans inside SCENE_YAW +/- SCENE_ARC degrees (a 180 degree arc), never spins
SCENE_IDLE = 0.7        # the idle ease swings through this fraction of the arc on each side
SCENE_ZOOM = 0.85       # the scene is drawn at this fraction of the size that fits, centred
SCENE_SX, SCENE_PY = 0.87, 0.36   # screen x per cell, screen y per cell on the ground (fixed pitch)
TOWER_CELLS, TOWER_PITCH = 2, 3   # footprint of a tower and distance between towers, in cells
SCENE_MX, SCENE_MY = 2.2, 1.8     # ground plate margin around the tower row, in cells
SCENE_GHOST = ("wait", "draft")   # layers not reviewed yet are drawn as ghost voxels
SCENE_ROWS = (16, 70)   # first text baseline of the upper and lower label rows
SCENE_BAND_BOTTOM = 106  # bottom of the lower label row
CHART_W, CHART_H = 280, 150   # viewBox of the trend charts: close to their usual box, so the text keeps its size
SCENE_STATES = [("done", "Looks done", "--good"), ("rev", "Reviewed", "--accent"),
                ("wait", "Waiting for review", "--partial"), ("draft", "Draft", "--soft")]


def top_areas(an, n=TOP_AREAS):
    """The n areas with the most open PRs (ties by name) as [{"name", "prs", "members"}], plus one rolled-up
    "Everything else" entry (members = the areas in it, sorted like the ranking) when there are more than n areas.
    members is None for a real area."""
    ranked = sorted(an["areas"], key=lambda a: (-len(an["areas"][a]), a))
    out = [{"name": a, "prs": an["areas"][a], "members": None} for a in ranked[:n]]
    rest = ranked[n:]
    if rest:
        out.append({"name": ELSE_NAME, "prs": [p for a in rest for p in an["areas"][a]], "members": rest})
    return out


def effort_scene(an, n=TOP_AREAS, cap=SCENE_CAP):
    """Tower data for the effort map: one tower per top_areas entry, layers bottom to top (looks done, reviewed,
    waiting, draft), h = voxel layers of a run. Above cap layers for the tallest tower, h is scaled by cap/tallest
    (every non-empty run keeps at least one layer) and per = open PRs per layer is stated in the caption."""
    entries = top_areas(an, n)
    tallest = max([len(e["prs"]) for e in entries] or [0])
    factor = cap / tallest if tallest > cap else 1.0
    towers = []
    for e in entries:
        ps = e["prs"]
        layers = []
        for k, label, _ in SCENE_STATES:
            c = sum(bucket(p) == k for p in ps)
            if c:
                layers.append({"k": k, "label": label, "n": c, "ghost": k in SCENE_GHOST,
                               "h": c if factor == 1.0 else max(1, round(c * factor))})
        towers.append({"name": e["name"], "members": e["members"] or [], "open": len(ps),
                       "unreviewed": sum(bool(p["unreviewed"]) for p in ps),
                       "add": sum(p.get("additions", 0) for p in ps), "del": sum(p.get("deletions", 0) for p in ps),
                       "layers": layers, "height": sum(ly["h"] for ly in layers)})
    per = tallest / cap if factor != 1.0 else 1.0
    if not towers:
        caption = ""
    elif factor == 1.0:
        caption = "One layer is one open PR."
    else:
        caption = "One layer is about %s open PRs: the layers are scaled so the tallest tower fits in %d." % (
            ("%.1f" % per).rstrip("0").rstrip("."), cap)
    if caption:
        caption += " Solid layers have moved forward (looks done, reviewed); outlined ghost layers still wait for review or are drafts."
    return {"towers": towers, "scale": factor, "per": per, "cap": cap, "caption": caption}


def scene_plate(n):
    """Half sizes (gx, gy) of the ground plate in cells for n towers."""
    hw = TOWER_CELLS / 2.0
    return (n - 1) * TOWER_PITCH / 2.0 + hw + SCENE_MX, hw + SCENE_MY


def yaw_range(yaw=SCENE_YAW, arc=SCENE_ARC):
    """(low, high) in degrees: the canvas yaw never leaves this range (the JS clampYaw does the same)."""
    return yaw - arc, yaw + arc


def clamp_yaw(deg, yaw=SCENE_YAW, arc=SCENE_ARC):
    lo, hi = yaw_range(yaw, arc)
    return max(lo, min(hi, deg))


def scene_summary(scene):
    ts = scene["towers"]
    return "Effort map, open pull requests per area: " + "; ".join(
        "%s %d open, %d unreviewed" % (t["name"], t["open"], t["unreviewed"]) for t in ts) + "."


def scene_json(scene):
    """Inline JSON for the canvas renderer; every '<' is escaped so nothing can close the script block."""
    data = {"towers": scene["towers"], "per": scene["per"], "caption": scene["caption"],
            "states": [{"k": k, "label": lb, "v": v, "ghost": k in SCENE_GHOST} for k, lb, v in SCENE_STATES],
            "geom": {"w": TOWER_CELLS, "pitch": TOWER_PITCH, "sx": SCENE_SX, "py": SCENE_PY, "yaw": SCENE_YAW,
                     "arc": SCENE_ARC, "idle": SCENE_IDLE, "zoom": SCENE_ZOOM, "gx": scene_plate(len(scene["towers"]))[0],
                     "gy": scene_plate(len(scene["towers"]))[1]}}
    return json.dumps(data, separators=(",", ":")).replace("<", "\\u003c")


def _num(v):
    return "%.1f" % v if abs(v - round(v)) > 0.05 else "%d" % round(v)


def _short(v):
    return "%.1fk" % (v / 1000.0) if v >= 10000 else (str(v) if v < 1000 else "%.1fk" % (v / 1000.0))


def _pj(x, y, z, yaw, k, lh):
    c, s = math.cos(yaw), math.sin(yaw)
    xr, yr = x * c - y * s, x * s + y * c
    return ((xr - yr) * SCENE_SX * k, (xr + yr) * SCENE_PY * k - z * lh, xr + yr)


_FACE_CORNERS = [(-1, -1), (1, -1), (1, 1), (-1, 1)]
_FACE_NORMALS = [(0, -1), (1, 0), (0, 1), (-1, 0)]


def _visible_faces(yaw):
    """[(corner index, 'l' or 'r')] of the side faces that look at the camera (same rule as the canvas)."""
    c, s = math.cos(yaw), math.sin(yaw)
    out = []
    for f, (nx, ny) in enumerate(_FACE_NORMALS):
        xr, yr = nx * c - ny * s, nx * s + ny * c
        if xr + yr > 0.001:
            out.append((f, "l" if xr - yr < 0 else "r"))
    return out


def scene_svg(scene):
    """Static isometric SVG of the scene at the default angle, transparent (the page gradient shows through): a faint
    ground plate, shaded squares (classes use the theme tokens), one <g class="tower"> per tower, callouts with
    leader lines. Empty string when there are no towers."""
    ts = scene["towers"]
    n = len(ts)
    if not n:
        return ""
    yaw = math.radians(SCENE_YAW)
    hw = TOWER_CELLS / 2.0
    xs = [(i - (n - 1) / 2.0) * TOWER_PITCH for i in range(n)]
    gx, gy = scene_plate(n)
    maxl = max([t["height"] for t in ts] or [0])
    pad, band_bottom = 12, SCENE_BAND_BOTTOM
    ground = [(-gx, -gy), (gx, -gy), (gx, gy), (-gx, gy)]

    def bbox(k, lh):
        pts = [_pj(x, y, 0, yaw, k, lh) for x, y in ground] + [
            _pj(xs[i] + cx * hw, cy * hw, t["height"], yaw, k, lh) for i, t in enumerate(ts) for cx, cy in _FACE_CORNERS]
        return min(p[0] for p in pts), max(p[0] for p in pts), min(p[1] for p in pts), max(p[1] for p in pts)

    # width fixes the scale; the layer height then takes all the height left under the callouts
    gxs = [_pj(x, y, 0, yaw, 1.0, 0)[0] for x, y in ground]
    k = (SCENE_W - 2 * pad) / (max(gxs) - min(gxs))
    avail = SCENE_H - band_bottom - 18
    while True:
        lh = k * 0.9
        while lh > k * 0.25:
            x0, x1, y0, y1 = bbox(k, lh)
            if y1 - y0 <= avail:
                break
            lh *= 0.97
        if lh > k * 0.25 or k < 3:
            break
        k *= 0.97
    k, lh = k * SCENE_ZOOM, lh * SCENE_ZOOM
    x0, x1, y0, y1 = bbox(k, lh)
    top, bot = band_bottom + 8, SCENE_H - 8
    ox, oy = SCENE_W / 2.0 - (x0 + x1) / 2.0, top + (bot - top - (y1 - y0)) / 2.0 - y0

    def P(x, y, z):
        px, py, depth = _pj(x, y, z, yaw, k, lh)
        return px + ox, py + oy, depth

    def pt(p):
        return "%.1f,%.1f" % (p[0], p[1])

    o = ['<svg class="scene-svg" viewBox="0 0 %d %d" preserveAspectRatio="xMidYMax meet" role="img" aria-label="%s">' % (
        SCENE_W, SCENE_H, esc(scene_summary(scene)))]
    gp = [P(x, y, 0) for x, y in ground]
    o.append('<polygon class="plate" points="%s"/>' % " ".join(pt(p) for p in gp))
    faces = _visible_faces(yaw)
    items = [(P(xs[i], 0, 0)[2], i) for i in range(n)]
    tops = {}

    def run_seams(i, ax, ay, bx, by, z0, z1):
        d = ["M%s L%s" % (pt(P(xs[i] + ax * hw, ay * hw, zz)), pt(P(xs[i] + bx * hw, by * hw, zz))) for zz in range(z0, z1 + 1)]
        for c in range(1, TOWER_CELLS):
            f0 = c / float(TOWER_CELLS)
            mx, my = ax + (bx - ax) * f0, ay + (by - ay) * f0
            d.append("M%s L%s" % (pt(P(xs[i] + mx * hw, my * hw, z0)), pt(P(xs[i] + mx * hw, my * hw, z1))))
        return " ".join(d)

    for _, i in sorted(items):
        t = ts[i]
        o.append('<g class="tower" data-area="%s">' % esc(t["name"]))
        for f, side in faces:
            (ax, ay), (bx, by) = _FACE_CORNERS[f], _FACE_CORNERS[(f + 1) % 4]
            z = 0
            for ly in t["layers"]:
                z1 = z + ly["h"]
                quad = [P(xs[i] + ax * hw, ay * hw, z), P(xs[i] + bx * hw, by * hw, z),
                        P(xs[i] + bx * hw, by * hw, z1), P(xs[i] + ax * hw, ay * hw, z1)]
                tip = "%s · %s · %d PR%s" % (t["name"], ly["label"].lower(), ly["n"], "" if ly["n"] == 1 else "s")
                o.append('<polygon class="vx v-%s f-%s%s" points="%s" data-tip="%s"/>' % (
                    ly["k"], side, " ghost" if ly["ghost"] else "", " ".join(pt(p) for p in quad), esc(tip)))
                o.append('<path class="%s v-%s" d="%s"/>' % ("gseam" if ly["ghost"] else "seam", ly["k"], run_seams(i, ax, ay, bx, by, z, z1)))
                z = z1
        if t["layers"]:
            top = t["layers"][-1]
            quad = [P(xs[i] + cx * hw, cy * hw, t["height"]) for cx, cy in _FACE_CORNERS]
            tip = "%s · %s · %d PR%s" % (t["name"], top["label"].lower(), top["n"], "" if top["n"] == 1 else "s")
            o.append('<polygon class="vx v-%s f-t%s" points="%s" data-tip="%s"/>' % (
                top["k"], " ghost" if top["ghost"] else "", " ".join(pt(p) for p in quad), esc(tip)))
        o.append("</g>")
        tx, ty, _ = P(xs[i], 0, t["height"])
        tops[i] = (tx, ty)
    # callouts in two staggered rows: a label's text lies between the leader lines of its neighbours in the other row
    sw = (SCENE_W - 2 * pad) / float(n)
    cs = [pad + (i + 0.5) * sw for i in range(n)]
    o.append('<g class="callouts">')
    for i, t in enumerate(ts):
        low = (n - 1 - i) % 2 == 0
        if low:
            ra, rb = (cs[i - 1] + 5 if i >= 1 else pad), (cs[i + 1] - 6 if i + 1 < n else SCENE_W - pad)
        else:
            ra, rb = cs[i] + 5, (cs[i + 2] - 6 if i + 2 < n else SCENE_W - pad)
        room = rb - ra
        name, shown = t["name"], t["name"]
        while len(shown) > 1 and len(shown) * 7.8 > room:
            shown = shown[:-1]
        if shown != name:
            shown = shown[:-1] + "\u2026"
        l2 = "%d open \u00b7 %d unreviewed" % (t["open"], t["unreviewed"])
        l2_s = "%d open \u00b7 %d unrev." % (t["open"], t["unreviewed"])
        if len(l2) * 6.4 > room:
            l2 = l2_s
        w = max(len(shown) * 7.8, len(l2) * 6.4)
        y0 = SCENE_ROWS[1] if low else SCENE_ROWS[0]
        x = min(max(ra, cs[i] - 18), rb - w) if low else ra
        x = max(ra, x) if low else x
        tx, ty = tops[i]
        if low:
            xe = min(max(cs[i], x + 4), x + w - 4)
            pts = "%.1f,%.1f %.1f,%d" % (tx, ty - 3, xe, y0 + 36)
        else:
            pts = "%.1f,%.1f %.1f,%d %.1f,%d" % (tx, ty - 3, cs[i], band_bottom + 8, cs[i], y0 - 11)
        o.append('<g class="callout"><title>%s</title><polyline points="%s"/><rect class="co-bg" x="%.1f" y="%d" width="%.1f" height="48"/>' % (
            esc("%s: %d open, %d unreviewed" % (name, t["open"], t["unreviewed"])), pts, x - 4, y0 - 13, w + 8))
        o.append('<text x="%.1f" y="%d"><tspan class="co-nm" x="%.1f">%s</tspan>'
                 '<tspan class="co-n" x="%.1f" dy="16"><tspan class="co-num">%d</tspan><tspan class="co-w"> open</tspan>'
                 '<tspan class="co-sub">%s</tspan></tspan>'
                 '<tspan class="co-sub" x="%.1f" dy="15"><tspan class="add">+%s</tspan> <tspan class="del">\u2212%s</tspan></tspan></text></g>' % (
                     x, y0, x, esc(shown), x, t["open"], esc(l2[len("%d open" % t["open"]):]), x, _short(t["add"]), _short(t["del"])))
    o.append("</g></svg>")
    return "".join(o)


SCENE_CSS = """
.scene{position:relative;display:flex;flex-direction:column;min-width:0;min-height:0;container-type:inline-size}
.stage{position:relative;flex:none;width:100%;height:min(60vh,420px)}
.scene-svg,.scene-cv{display:block;position:absolute;left:0;top:0;width:100%;height:100%}
.scene-cv{touch-action:pan-y;cursor:grab;outline-offset:-2px}.scene-cv:active{cursor:grabbing}
.scene .score-box{margin-bottom:6px}
.scene-foot{display:flex;flex-wrap:wrap;align-items:center;gap:2px 14px;margin-top:4px}
.plate{fill:var(--ink);fill-opacity:.07;stroke:var(--ink);stroke-opacity:.2;stroke-width:1;stroke-linejoin:round}
.co-bg{fill:var(--surface)}
.vx{--c:var(--ink)}.v-done{--c:var(--good)}.v-rev{--c:var(--accent)}.v-wait{--c:var(--partial)}.v-draft{--c:var(--soft)}
.vx{fill:var(--c);stroke:none}
.f-t{fill:color-mix(in srgb,var(--c) 68%,#fff)}.f-l{fill:var(--c)}.f-r{fill:color-mix(in srgb,var(--c) 66%,#000)}
.vx.ghost{fill:var(--c);fill-opacity:.16;stroke:var(--c);stroke-width:1;stroke-linejoin:round}
.seam{fill:none;stroke:var(--page);stroke-width:.7;opacity:.85}.gseam{fill:none;stroke:var(--c);stroke-width:.7;opacity:.5}
.callout polyline{fill:none;stroke:var(--ink);stroke-width:1}
.scene-svg text{font:400 12px Roboto,Arial,sans-serif;fill:var(--ink);paint-order:stroke;stroke:var(--page);stroke-width:3px;stroke-linejoin:round}
.scene-svg .co-nm{font:700 14px "Roboto Condensed",Arial,sans-serif;fill:var(--ink)}.scene-svg .co-n{font-weight:500;fill:var(--ink)}
.scene-svg .add{fill:var(--good)}.scene-svg .del{fill:var(--bad)}
.scene-cap{color:var(--soft);font-size:13px;line-height:1.35;flex:1 1 240px;min-width:0}
.scene .legend{margin:0}
.scene .legend .sw.k-wait{background:color-mix(in srgb,var(--partial) 18%,transparent);box-shadow:inset 0 0 0 1px var(--partial)}
.scene .legend .sw.k-draft{background:color-mix(in srgb,var(--soft) 18%,transparent);box-shadow:inset 0 0 0 1px var(--soft)}
.scene-key{display:none;margin:6px 0 0}.scene-key li{display:flex;justify-content:space-between;gap:8px;padding:4px 0;border-top:1px solid var(--rule);font-size:13px}
.scene-key small{color:var(--soft)}
@media (min-width:900px){.stage{flex:1;height:auto;min-height:0}.scene-cap{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
@container (max-width:360px){
.scene-svg .co-nm,.scene-svg .co-sub,.scene-svg .co-w,.scene-svg .co-bg{display:none}.scene-svg .co-n{font:700 30px "Roboto Condensed",Arial,sans-serif}
.scene-key{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));column-gap:12px}}
@media (max-width:899px){@container (max-width:540px){
.scene-svg .co-nm,.scene-svg .co-sub,.scene-svg .co-w,.scene-svg .co-bg{display:none}.scene-svg .co-n{font:700 30px "Roboto Condensed",Arial,sans-serif}
.scene-key{display:block}}}
"""

SCENE_JS = r"""
(function(){
var d=document,stage=d.getElementById('stage'),dEl=d.getElementById('scene-data'),tip=d.getElementById('tip');
if(!stage||!dEl)return;
var svg=stage.querySelector('svg'),data;
try{data=JSON.parse(dEl.textContent);}catch(e){return;}
var T=data.towers||[],N=T.length,S=data.states||[],G=data.geom||{};
if(!N||!svg)return;
var cv=d.createElement('canvas'),g=cv.getContext&&cv.getContext('2d');
if(!g)return;
cv.className='scene-cv';cv.tabIndex=0;cv.setAttribute('role','img');
cv.setAttribute('aria-label',(svg.getAttribute('aria-label')||'Effort map')+' Drag or press Left and Right to pan.');
stage.replaceChild(cv,svg);
// Layers: layer (ground plate, towers, labels; rebuilt when the yaw or the size changes), cv (transparent composite with the hover highlight).
// The page gradient shows through both canvases: there is no backdrop to paint.
function mk(){var c=d.createElement('canvas');return[c,c.getContext('2d')];}
var Y=mk(),yl=Y[0],lx=Y[1];
var TW=G.w,P=G.pitch,SX=G.sx,PY=G.py,HX=G.gx||8,HY=G.gy||3,Y0=G.yaw*Math.PI/180,ARC=(G.arc==null?90:G.arc)*Math.PI/180,IDLE=G.idle==null?.7:G.idle,ZOOM=G.zoom||.85;
var yaw=Y0;
var W=0,H=0,dpr=1,K=0,LH=5,CX=0,CY=0,bandTop=0,bandBottom=0,compact=false,short=false;
var hover=null,hits=[],labelHits=[],col={},mc={};
var reduce=!!(window.matchMedia&&matchMedia('(prefers-reduced-motion: reduce)').matches);
var auto=!reduce,raf=0,last=0,lastDraw=-1e9,ph=0,inside=false,drag=null,vis=true;
var sceneDirty=true,rz=false,sceneYaw=NaN,ordYaw=NaN,ord=[],VF=[];
var Q=.008,PERIOD=42000,MS=30;
// the pan stays inside Y0 +/- ARC (a 180 degree arc): drag, keys and the idle ease all go through clampYaw
function clampYaw(v){return Math.max(Y0-ARC,Math.min(Y0+ARC,v));}
function hex(s){var m=/^#([0-9a-f]{6})$/i.exec((s||'').trim());if(!m)return[128,128,128];var n=parseInt(m[1],16);return[n>>16&255,n>>8&255,n&255];}
function mix(c,t,a){return[0,1,2].map(function(i){return Math.round(c[i]+(t[i]-c[i])*a);});}
function rgb(c){return'rgb('+c[0]+','+c[1]+','+c[2]+')';}
function rgba(c,a){return'rgba('+c[0]+','+c[1]+','+c[2]+','+a+')';}
function readColors(){var cs=getComputedStyle(d.documentElement);function v(n){return hex(cs.getPropertyValue(n));}
var ink=v('--ink');
col={ink:rgb(ink),page:rgb(v('--page')),soft:rgb(v('--soft')),good:rgb(v('--good')),bad:rgb(v('--bad')),st:{},surface:(cs.getPropertyValue('--surface')||'').trim()||'rgba(255,255,255,.78)',plate:rgba(ink,.07),plateEdge:rgba(ink,.2)};
var W3=[255,255,255],B3=[0,0,0];
S.forEach(function(s){var c=v(s.v);col.st[s.k]={ghost:!!s.ghost,c:rgb(c),g:rgba(c,.16),gh:rgba(c,.2),l:rgb(c),t:rgb(mix(c,W3,.32)),r:rgb(mix(c,B3,.34)),
ht:rgb(mix(c,W3,.6)),hl:rgb(mix(c,W3,.38)),hr:rgb(mix(c,W3,.16))};});}
function ext(y,k,lh,gOnly){var c=Math.cos(y),s=Math.sin(y),r=[1e9,-1e9,1e9,-1e9],h=TW/2;
function add(px,py,z){var xr=px*c-py*s,yr=px*s+py*c,sx=(xr-yr)*SX*k,sy=(xr+yr)*PY*k-z*lh;
r[0]=Math.min(r[0],sx);r[1]=Math.max(r[1],sx);r[2]=Math.min(r[2],sy);r[3]=Math.max(r[3],sy);}
[[-HX,-HY],[HX,-HY],[HX,HY],[-HX,HY]].forEach(function(q){add(q[0],q[1],0);});
if(!gOnly)T.forEach(function(t,i){var x0=(i-(N-1)/2)*P;[[-h,-h],[h,-h],[h,h],[-h,h]].forEach(function(q){add(x0+q[0],q[1],t.height);});});
return r;}
// vertical extent over the whole pan (samples every 15 degrees): one fixed CY keeps the scene from bobbing while it pans
function extArc(k,lh){var r=[0,0,1e9,-1e9],a,e;for(a=-ARC;a<=ARC+1e-6;a+=Math.PI/12){e=ext(Y0+a,k,lh);r[2]=Math.min(r[2],e[2]);r[3]=Math.max(r[3],e[3]);}return r;}
// Sizes follow the stage box; returns false when nothing changed (no canvas is resized for nothing).
function layout(){
var r=stage.getBoundingClientRect(),w=Math.max(200,Math.round(r.width)),h=Math.max(160,Math.round(r.height)),dp=Math.min(2,window.devicePixelRatio||1);
if(K&&w===W&&h===H&&dp===dpr)return false;
W=w;H=h;dpr=dp;
[cv,yl].forEach(function(c){c.width=Math.round(W*dpr);c.height=Math.round(H*dpr);});
compact=W<=360||(W<=540&&innerWidth<900);short=!compact&&H<300;bandTop=4;bandBottom=bandTop+(compact?28:short?40:102);
var mh=0,a,e;for(a=0;a<Math.PI;a+=Math.PI/36){e=ext(a,1,0,true);mh=Math.max(mh,Math.abs(e[0]),Math.abs(e[1]));}
var avail=H-bandBottom-18;K=(W-16)/(2*mh);
for(;;){LH=K*.9;while(LH>K*.25){e=extArc(K,LH);if(e[3]-e[2]<=avail)break;LH*=.97;}if(LH>K*.25||K<3)break;K*=.97;}
K*=ZOOM;LH*=ZOOM;e=extArc(K,LH);CX=W/2;CY=bandBottom+8+(H-6-bandBottom-8-(e[3]-e[2]))/2-e[2];
sceneDirty=true;return true;}
function pj(x,y,z){var c=Math.cos(sceneYaw),s=Math.sin(sceneYaw),xr=x*c-y*s,yr=x*s+y*c;return[CX+(xr-yr)*SX*K,CY+(xr+yr)*PY*K-z*LH,xr+yr];}
var CORN=[[-1,-1],[1,-1],[1,1],[-1,1]],NORM=[[0,-1],[1,0],[0,1],[-1,0]];
// ---- layer: everything that depends on the yaw (the ground plate turns with it). Painter order and visible faces are cached per yaw.
function order(){if(ordYaw===sceneYaw)return;ordYaw=sceneYaw;var c=Math.cos(sceneYaw),s=Math.sin(sceneYaw);VF=[];
NORM.forEach(function(n,f){var xr=n[0]*c-n[1]*s,yr=n[0]*s+n[1]*c;if(xr+yr>.001)VF.push([f,xr-yr<0?'l':'r']);});
ord=[];var i;for(i=0;i<N;i++)ord.push({d:pj((i-(N-1)/2)*P,0,0)[2],i:i});
ord.sort(function(a,b){return a.d-b.d;});}
// a unit square (u,v) mapped onto the parallelogram (a; a+u; a+v): a face, a ground plate or a tower top is one fillRect
function tf(x,ux,uy,vx,vy,ox,oy){x.setTransform(dpr*ux,dpr*uy,dpr*vx,dpr*vy,dpr*ox,dpr*oy);}
function path(x,q){x.beginPath();q.forEach(function(p,i){if(i)x.lineTo(p[0],p[1]);else x.moveTo(p[0],p[1]);});x.closePath();}
function bbox(q){var a=1e9,b=1e9,c=-1e9,e=-1e9;q.forEach(function(p){a=Math.min(a,p[0]);b=Math.min(b,p[1]);c=Math.max(c,p[0]);e=Math.max(e,p[1]);});return[a,b,c,e];}
function drawGround(x){var q=[[-HX,-HY],[HX,-HY],[HX,HY],[-HX,HY]],gq=q.map(function(c){return pj(c[0],c[1],0);});
x.setTransform(dpr,0,0,dpr,0,0);path(x,gq);x.fillStyle=col.plate;x.fill();x.strokeStyle=col.plateEdge;x.lineWidth=1;x.lineJoin='round';x.stroke();}
// grid of one face run (z0..z1): the horizontal seams are one dashed stroke down the face, each vertical seam is a thin fillRect
function grid(x,z0,z1,colour,alpha,us,ex){var dv=.75/Math.max(.5,Math.abs(ex)),dl=.75/LH;x.globalAlpha=alpha;
if(LH>=2.2){x.strokeStyle=colour;x.lineWidth=1;x.setLineDash([dl,1-dl]);x.beginPath();x.moveTo(.5,z0-dl/2);x.lineTo(.5,z1+dl*.6);x.stroke();x.setLineDash([]);}
x.fillStyle=colour;us.forEach(function(u){x.fillRect(u-dv/2,z0,dv,z1-z0);});x.globalAlpha=1;}
function drawTower(x,i){var t=T[i],L=t.layers,nl=L.length;if(!nl)return;
var x0=(i-(N-1)/2)*P,h=TW/2,z=[0],k,ns=0;for(k=0;k<nl;k++){z.push(z[k]+L[k].h);if(!col.st[L[k].k].ghost&&ns===k)ns=k+1;}
VF.forEach(function(F){var a=CORN[F[0]],b=CORN[(F[0]+1)%4],p0=pj(x0+a[0]*h,a[1]*h,0),p1=pj(x0+b[0]*h,b[1]*h,0),ex=p1[0]-p0[0],ey=p1[1]-p0[1];
tf(x,ex,ey,0,-LH,p0[0],p0[1]);
for(k=0;k<nl;k++){var cc=col.st[L[k].k];x.fillStyle=cc.ghost?cc.g:cc[F[1]];x.fillRect(0,z[k],1,L[k].h);
var q=[[p0[0],p0[1]-z[k]*LH],[p1[0],p1[1]-z[k]*LH],[p1[0],p1[1]-z[k+1]*LH],[p0[0],p0[1]-z[k+1]*LH]];
hits.push({q:q,bb:bbox(q),t:i,l:k,s:F[1],m:[dpr*ex,dpr*ey,0,-dpr*LH,dpr*p0[0],dpr*p0[1]],r:[0,z[k],1,L[k].h]});}
if(ns)grid(x,0,z[ns],col.page,.8,[.5],ex);
if(ns<nl)grid(x,z[ns],z[nl],col.ink,.4,[0,.5,1],ex);});
var cc=col.st[L[nl-1].k],c0=pj(x0-h,-h,z[nl]),c1=pj(x0+h,-h,z[nl]),c2=pj(x0+h,h,z[nl]),c3=pj(x0-h,h,z[nl]),q=[c0,c1,c2,c3];
tf(x,c1[0]-c0[0],c1[1]-c0[1],c3[0]-c0[0],c3[1]-c0[1],c0[0],c0[1]);x.fillStyle=cc.ghost?cc.g:cc.t;x.fillRect(0,0,1,1);
hits.push({q:q,bb:bbox(q),t:i,l:nl-1,s:'t',m:[dpr*(c1[0]-c0[0]),dpr*(c1[1]-c0[1]),dpr*(c3[0]-c0[0]),dpr*(c3[1]-c0[1]),dpr*c0[0],dpr*c0[1]],r:[0,0,1,1]});
if(cc.ghost){x.strokeStyle=cc.c;x.lineWidth=2/(Math.hypot(c1[0]-c0[0],c1[1]-c0[1])+Math.hypot(c3[0]-c0[0],c3[1]-c0[1]));x.strokeRect(0,0,1,1);}}
function fitText(x,f,s,w){var k=f+'|'+s+'|'+Math.round(w),v=mc[k];if(v!==undefined)return v;x.font=f;var o=s;
if(x.measureText(o).width>w){while(o.length>1&&x.measureText(o+'…').width>w)o=o.slice(0,-1);o+='…';}return mc[k]=o;}
function tw(x,f,s){var k='w|'+f+'|'+s,v=mc[k];if(v!==undefined)return v;x.font=f;return mc[k]=x.measureText(s).width;}
function txt(x,s,px,py,f){x.lineWidth=3;x.lineJoin='round';x.strokeStyle=col.page;x.strokeText(s,px,py);x.fillStyle=f;x.fillText(s,px,py);}
function sh(v){return v<1000?String(v):(v/1000).toFixed(1)+'k';}
var F1='700 14px "Roboto Condensed",Arial,sans-serif',F2='500 12px Roboto,Arial,sans-serif',F3='400 12px Roboto,Arial,sans-serif',F4='700 22px "Roboto Condensed",Arial,sans-serif';
// callouts in two staggered rows (one row on narrow screens); each text lies between the leaders of the other row
function drawLabels(x){x.setTransform(dpr,0,0,dpr,0,0);x.textBaseline='alphabetic';labelHits=[];
var tops=T.map(function(t,i){var p=pj((i-(N-1)/2)*P,0,t.height);return{i:i,x:p[0],y:p[1]};});tops.sort(function(a,b){return a.x-b.x;});
var swd=(W-16)/N,cs=tops.map(function(o,k){return 8+(k+.5)*swd;}),L=[];
tops.forEach(function(o,k){var t=T[o.i],c=cs[k],low=!compact&&!short&&(N-1-k)%2===0,ra,rb;
if(compact||short){ra=c+5;rb=k+1<N?c+swd-6:W-8;}else if(low){ra=k>=1?cs[k-1]+5:8;rb=k+1<N?cs[k+1]-6:W-8;}else{ra=c+5;rb=k+2<N?cs[k+2]-6:W-8;}
var y0=bandTop+(compact?22:low?66:12),px=ra,w=0,nm=t.name,l2,l3;
if(compact){w=tw(x,F4,String(t.open));}
else{nm=fitText(x,F1,t.name,rb-ra);w=tw(x,F1,nm);l2=t.open+' open · '+t.unreviewed+' unreviewed';if(tw(x,F2,l2)>rb-ra)l2=t.open+' open · '+t.unreviewed+' unrev.';
l2=fitText(x,F2,l2,rb-ra);w=Math.max(w,tw(x,F2,l2));if(!short){l3='+'+sh(t.add)+' −'+sh(t['del']);w=Math.max(w,tw(x,F2,l3));}}
if(low)px=Math.max(ra,Math.min(c-18,rb-w));
L.push({o:o,t:t,c:c,low:low,y0:y0,x:px,w:w,nm:nm,l2:l2});});
x.beginPath();L.forEach(function(e){x.moveTo(e.o.x,e.o.y-3);
if(e.low)x.lineTo(Math.min(Math.max(e.c,e.x+4),e.x+e.w-4),e.y0+36);else{x.lineTo(e.c,bandBottom+(compact?4:8));x.lineTo(e.c,e.y0-(compact?20:11));}});
x.strokeStyle=col.ink;x.lineWidth=1;x.stroke();
L.forEach(function(e){var t=e.t,bh=compact?26:short?34:48;x.fillStyle=col.surface;x.fillRect(e.x-4,e.y0-13,e.w+8,bh);labelHits.push({x:e.x,y:e.y0-12,w:e.w,h:compact?26:short?34:48,i:e.o.i});
if(compact){x.font=F4;txt(x,String(t.open),e.x,e.y0,col.ink);return;}
x.font=F1;txt(x,e.nm,e.x,e.y0,col.ink);x.font=F2;txt(x,e.l2,e.x,e.y0+16,col.ink);
if(short)return;x.font=F3;var pl='+'+sh(t.add);txt(x,pl,e.x,e.y0+31,col.good);txt(x,'−'+sh(t['del']),e.x+tw(x,F3,pl+' '),e.y0+31,col.bad);});}
function buildScene(){sceneDirty=false;sceneYaw=yaw;hits=[];order();
var x=lx;x.setTransform(1,0,0,1,0,0);x.clearRect(0,0,yl.width,yl.height);
drawGround(x);
ord.forEach(function(it){drawTower(x,it.i);});
drawLabels(x);x.setTransform(1,0,0,1,0,0);x.globalAlpha=1;}
// ---- composite: the only thing a normal frame does. The canvas is cleared, so the page gradient shows through.
function composite(){g.setTransform(1,0,0,1,0,0);g.clearRect(0,0,cv.width,cv.height);g.drawImage(yl,0,0);
if(hover){hits.forEach(function(h){if(h.t!==hover.t||h.l!==hover.l)return;var cc=col.st[T[h.t].layers[h.l].k];
g.setTransform(h.m[0],h.m[1],h.m[2],h.m[3],h.m[4],h.m[5]);g.fillStyle=cc.ghost?cc.gh:cc['h'+h.s];g.fillRect(h.r[0],h.r[1],h.r[2],h.r[3]);});g.setTransform(1,0,0,1,0,0);}}
function shown(){return vis&&!d.hidden;}
// only the idle pan animates: it runs until the first user input and pauses while the pointer is over the map
function animating(){return auto&&!reduce&&!inside&&!drag;}
function sched(){if(!raf&&shown())raf=requestAnimationFrame(frame);}
function frame(ts){raf=0;
if(!shown()){last=0;return;}
if(ts-lastDraw<MS){raf=requestAnimationFrame(frame);return;}
var dt=last?Math.min(100,ts-last):0;last=ts;lastDraw=ts;
if(rz){rz=false;layout();}
if(animating()){ph+=dt*2*Math.PI/PERIOD;yaw=clampYaw(Y0+Math.round(Math.sin(ph)*ARC*IDLE/Q)*Q);}
if(sceneDirty||sceneYaw!==yaw)buildScene();
composite();
if(animating())sched();else last=0;}
function pos(e){var r=cv.getBoundingClientRect();return[e.clientX-r.left,e.clientY-r.top];}
function place(e){var x=e.clientX+12,y=e.clientY+14,w=tip.offsetWidth,h=tip.offsetHeight;
if(x+w>innerWidth-8)x=Math.max(8,e.clientX-w-12);if(y+h>innerHeight-8)y=Math.max(8,e.clientY-h-12);tip.style.left=x+'px';tip.style.top=y+'px';}
function inQ(q,px,py){var s=0;for(var i=0;i<q.length;i++){var a=q[i],b=q[(i+1)%q.length],c=(b[0]-a[0])*(py-a[1])-(b[1]-a[1])*(px-a[0]);
if(c!==0){if(s&&(c>0)!==(s>0))return false;s=c;}}return true;}
// hit-test on the polygons cached when the layer was built; the canvas is repainted only when the hovered layer changes
function pick(e){var p=pos(e),f=null,i;
for(i=hits.length-1;i>=0;i--){var h=hits[i],b=h.bb;if(p[0]<b[0]||p[0]>b[2]||p[1]<b[1]||p[1]>b[3])continue;if(inQ(h.q,p[0],p[1])){f=h;break;}}
var lab=null;if(!f)for(var j=0;j<labelHits.length;j++){var L=labelHits[j];if(p[0]>=L.x&&p[0]<=L.x+L.w&&p[1]>=L.y&&p[1]<=L.y+L.h){lab=T[L.i];break;}}
var ch=(f?f.t:-1)!==(hover?hover.t:-1)||(f?f.l:-1)!==(hover?hover.l:-1);hover=f?{t:f.t,l:f.l}:null;
if(f&&tip){var t=T[f.t],ly=t.layers[f.l];tip.textContent=t.name+' · '+ly.label.toLowerCase()+' · '+ly.n+' PR'+(ly.n===1?'':'s');tip.hidden=false;place(e);}
else if(lab&&tip){tip.textContent=lab.name+' · '+lab.open+' open · '+lab.unreviewed+' unreviewed';tip.hidden=false;place(e);}
else if(tip)tip.hidden=true;if(ch)sched();}
cv.addEventListener('pointerdown',function(e){drag={x:e.clientX};auto=false;if(cv.setPointerCapture)try{cv.setPointerCapture(e.pointerId);}catch(x){}});
cv.addEventListener('pointermove',function(e){inside=true;if(drag){yaw=clampYaw(yaw+(e.clientX-drag.x)*.01);drag.x=e.clientX;hover=null;if(tip)tip.hidden=true;sched();}else pick(e);});
function up(){drag=null;sched();}
cv.addEventListener('pointerup',up);cv.addEventListener('pointercancel',up);
cv.addEventListener('pointerenter',function(){inside=true;});
cv.addEventListener('pointerleave',function(){inside=false;hover=null;if(tip)tip.hidden=true;sched();});
cv.addEventListener('keydown',function(e){var k=e.key==='ArrowLeft'?-1:e.key==='ArrowRight'?1:0;if(!k)return;e.preventDefault();auto=false;yaw=clampYaw(yaw+k*.12);sched();});
// the size is re-read in the next frame, so a burst of resize events costs one layout
function resized(){rz=true;sched();}
if(window.ResizeObserver)new ResizeObserver(resized).observe(stage);else addEventListener('resize',resized);
if(window.IntersectionObserver)new IntersectionObserver(function(es){vis=es[es.length-1].isIntersecting;if(vis)sched();}).observe(cv);
d.addEventListener('visibilitychange',function(){if(!d.hidden)sched();});
function recolor(){readColors();sceneDirty=true;sched();}
if(window.MutationObserver)new MutationObserver(recolor).observe(d.documentElement,{attributes:true,attributeFilter:['data-theme']});
if(window.matchMedia)try{matchMedia('(prefers-color-scheme: dark)').addEventListener('change',recolor);}catch(e){}
if(d.fonts&&d.fonts.ready)d.fonts.ready.then(function(){mc={};sceneDirty=true;sched();});
readColors();layout();sched();
})();
"""


def render_html(an, repo, now, hist=None):
    hist = hist or []
    prs = an["prs"]
    by_num = {p["number"]: p for p in prs}
    hot_set = {h["file"] for h in an["hotFiles"]}
    areas = sorted(an["areas"], key=lambda a: (-len(an["areas"][a]), a))
    top = top_areas(an)
    scene = effort_scene(an)
    n_open, n_un, n_dr = len(prs), sum(p["unreviewed"] for p in prs), sum(bool(p["isDraft"]) for p in prs)
    n_done, n_stale = sum(p["looksDone"] for p in prs), sum(p["stale"] for p in prs)
    n_fail = sum(p["ci"] in ("FAILURE", "ERROR") for p in prs)
    n_clusters = len(an["clusters"])

    def plink(n):
        return '<a href="%s">#%d</a>' % (esc(safe_url(by_num[n]["url"])), n)

    def dl(key, cur):
        return change_24h(hist, key, cur, now)

    def pr_line(p):
        return '<a href="%s">#%d</a> %s' % (esc(safe_url(p["url"])), p["number"], esc(p["title"]))

    o = []
    ranked_top = [e for e in top if not e["members"]]
    else_entry = next((e for e in top if e["members"]), None)
    area_opts = ['<option value="">All areas</option>']
    for e in ranked_top:
        area_opts.append('<option value="%s">%s (%d)</option>' % (esc(slug(e["name"])), esc(e["name"]), len(e["prs"])))
    if else_entry:
        area_opts.append('<option value="%s" data-members="%s">%s (%d)</option>' % (
            esc(slug(ELSE_NAME)), esc(",".join(slug(m) for m in else_entry["members"])), esc(ELSE_NAME), len(else_entry["prs"])))
        area_opts.append('<optgroup label="Areas inside Everything else">%s</optgroup>' % "".join(
            '<option value="%s">%s (%d)</option>' % (esc(slug(m)), esc(m), len(an["areas"][m])) for m in else_entry["members"]))
    toolbar = ('<div class="toolbar" id="filters" role="search"><label class="vh" for="q">Search pull requests</label>'
               '<input id="q" type="search" placeholder="Search title, number, author or path" autocomplete="off">'
               '<label class="vh" for="area">Area</label><select id="area">%s</select>'
               '<div class="seg" role="group" aria-label="State">%s</div><span class="result" id="result" role="status"></span></div>' % (
                   "".join(area_opts),
                   "".join('<button type="button" data-state="%s" aria-pressed="%s">%s</button>' % (k, "true" if not k else "false", n)
                           for k, n in (("", "All"), ("unrev", "Unreviewed"), ("done", "Ready"), ("draft", "Draft")))))
    o.append("<!doctype html><html lang=en><head><meta charset=utf-8>"
             "<meta name=viewport content='width=device-width,initial-scale=1'>"
             "<title>PR scan: %s</title>"
             "<link rel=preconnect href='https://fonts.googleapis.com'><link rel=preconnect href='https://fonts.gstatic.com' crossorigin>"
             "<link rel=stylesheet href='%s'><script>%s</script><style>%s%s</style></head><body><div class=\"app\">" % (
                 esc(repo), FONTS_URL, THEME_JS, CSS, SCENE_CSS))
    o.append('<header class="top"><h1>PR scan <span>/ <a href="https://github.com/%s">%s</a></span></h1>%s'
             '<div class="top-right"><div class="live"><i class="dot" aria-hidden="true"></i>'
             '<span><span id="ago">Updated </span><time id="upd" datetime="%s">%s UTC</time></span></div>'
             '<button type="button" class="theme" id="theme" aria-label="Colour theme">Theme</button></div></header>' % (
                 esc(repo), esc(repo), toolbar, now.strftime(TS_FMT), now.strftime("%Y-%m-%d %H:%M")))

    # ---- city panel: score box + trends
    d_open = change_24h(hist, "open", n_open, now)
    o.append('<section class="a-city" aria-label="Effort map and trends"><div class="scene%s"><div class="score-box"><div class="score">%d<small>open</small></div>'
             '<p class="score-note"><b>%d unreviewed &middot; %d look done</b><span>%d possible duplicate group%s &middot; %d draft%s</span>%s</p></div>' % (
                 " has-stage" if scene["towers"] else "", n_open, n_un, n_done, n_clusters, "" if n_clusters == 1 else "s",
                 n_dr, "" if n_dr == 1 else "s", delta_text(d_open)))
    if scene["towers"]:
        cap = "Effort map. One tower per area, layer by PR. " + scene["caption"]
        o.append('<div class="stage" id="stage">%s</div><div class="scene-foot">%s<p class="scene-cap" title="%s"><b>Effort map.</b> One tower per area, layer by PR. %s</p></div>'
                 '<ol class="scene-key">%s</ol><script type="application/json" id="scene-data">%s</script>' % (
                     scene_svg(scene), legend([(n, "k-" + k) for k, n, _ in SCENE_STATES]), esc(cap), esc(scene["caption"]),
                     "".join('<li>%s<small>%d open</small></li>' % (
                         ptype(t["name"]) if not t["members"] else '<span class="ptype" style="--ac:var(--soft)"><i></i><span class="nm">%s</span></span>' % esc(t["name"]),
                         t["open"]) for t in scene["towers"]), scene_json(scene)))
    else:
        o.append('<p class="empty-block">No data yet</p><p class="sec-note">The effort map needs at least one open pull request.</p>')
    o.append("</div></section>")
    recent = window(hist, now, TREND_DAYS)
    o.append('<section class="a-trends" aria-label="Trends"><div class="head"><h2>Trends</h2><span>last %d days, %d snapshot%s</span></div>' % (
        TREND_DAYS, len(recent), "" if len(recent) == 1 else "s"))
    if len(recent) < 2:
        o.append('<p class="empty-block">No data yet</p><p class="sec-note">Trends start after the first hours of hourly snapshots (%d so far).</p>' % len(recent))
    else:
        days = [x for x in daily_summary(recent) if x["pairs"]] if len({x["ts"][:10] for x in recent}) >= 2 else []
        o.append('<div class="charts"><div class="chart-block"><div class="chart-head"><h3>Open and unreviewed</h3>%s</div><div class="chart-box">%s</div></div>' % (
            legend([("Open", "s-open"), ("Unreviewed", "s-unrev")]),
            svg_line_chart([
                {"name": "open", "cls": "s-open", "points": [(parse_time(x["ts"]), x["open"]) for x in recent]},
                {"name": "unreviewed", "cls": "s-unrev", "points": [(parse_time(x["ts"]), x["unreviewed"]) for x in recent]}],
                "Open and unreviewed pull requests over the last %d days" % TREND_DAYS, CHART_W, CHART_H)))
        o.append('<div class="chart-block"><div class="chart-head"><h3>Opened and closed per day</h3>%s</div><div class="chart-box">%s</div></div></div>' % (
            legend([("Opened", "b-new"), ("Closed or merged", "b-closed")]),
            svg_bars(days, "Pull requests opened and closed per day (UTC)", CHART_W, CHART_H)))
    o.append("</section>")

    # ---- road: PR lifecycle
    cnt_all = {k: sum(bucket(p) == k for p in prs) for k, _ in BUCKETS}
    merged24 = an.get("merged24")
    o.append('<section class="a-road" aria-label="Pull request lifecycle"><div class="head"><h2>Road to merged</h2><span>%d open</span></div><ol class="road">' % n_open)
    for (k, name), color in zip(BUCKETS, ("var(--soft)", "var(--partial)", "var(--accent)", "var(--good)")):
        o.append(road_step({"wait": "Waiting", "done": "Ready"}.get(k, name), cnt_all[k], cnt_all[k] / n_open if n_open else 0, color))
    o.append(road_step("Merged 24 h", merged24, None, "var(--good)", ""))
    o.append("</ol></section>")

    # ---- standings: areas ranked by open PRs
    spark_hist = window(hist, now, SPARK_DAYS)
    biggest = max([len(e["prs"]) for e in top] or [1])
    o.append('<section class="a-standings" aria-label="Areas"><div class="head"><h2>By area</h2><span>top %d by open PRs, line = %dd</span></div>'
             '<div style="margin-bottom:8px">%s</div><div class="st-list"><div class="st-axis" aria-hidden="true"><span>#</span><span>Area</span>'
             '<span>State mix</span><span>Open</span></div><ol>' % (TOP_AREAS, SPARK_DAYS, legend([(n, "k-" + k) for k, n in BUCKETS])))
    for i, e in enumerate(top, 1):
        a, ps, members = e["name"], e["prs"], e["members"]
        cnt = {k: sum(bucket(p) == k for p in ps) for k, _ in BUCKETS}
        segs = "".join('<i class="%s" style="flex:%d" data-tip="%s" title="%s"></i>' % (
            k, cnt[k], esc("%s: %d" % (n, cnt[k])), esc("%s: %d" % (n, cnt[k]))) for k, n in BUCKETS if cnt[k])
        ma = members or [a]
        series = downsample([sum(s["areas"].get(m, 0) for m in ma) for s in spark_hist if s["open"] == 0 or s["areas"]], 48)
        who = (ptype(a, link=True) if not members else '<span class="ptype" style="--ac:var(--soft)" title="%s"><i></i><span class="nm">%s</span></span>' % (
            esc(", ".join(members)), esc(a)))
        sub = (" · ".join("%d %s" % (cnt[k], n.lower()) for k, n in BUCKETS if cnt[k]) if not members else
               "%d areas: %s" % (len(members), ", ".join(members)))
        o.append('<li class="st"><span class="rank">%d</span><div class="who"><b>%s</b><small title="%s">%s</small></div>'
                 '<span class="track"><span class="fill" style="width:%.1f%%">%s</span></span>'
                 '<div class="where"><b>%d</b>%s</div></li>' % (
                     i, who, esc(sub), esc(sub),
                     100.0 * len(ps) / biggest, segs, len(ps),
                     sparkline(series, "%s: open PRs over %d days" % (a, SPARK_DAYS))))
    o.append("</ol></div></section>")

    # ---- feed: tabs
    done = sorted([p for p in prs if p["looksDone"]], key=lambda p: (-p["ageDays"], p["number"]))
    metas = {"dup": "%d group%s" % (n_clusters, "" if n_clusters == 1 else "s"), "ready": "%d, oldest first" % len(done),
             "hot": "%d file%s" % (len(an["hotFiles"]), "" if len(an["hotFiles"]) == 1 else "s"), "all": "%d pull requests" % n_open}
    titles = [("dup", "Duplicates"), ("ready", "Ready to merge"), ("hot", "Hot files"), ("all", "All PRs")]
    o.append('<section class="a-feed" aria-label="Pull request lists"><div class="head"><div class="tabs">%s</div><span id="tabmeta"></span></div><div class="feed-body" id="feed">' % "".join(
        '<button type="button" data-tab="%s" aria-pressed="%s">%s</button>' % (k, "true" if k == "dup" else "false", n) for k, n in titles))

    def panel(key, body, note=""):
        name = dict(titles)[key]
        return ('<div class="panel" data-panel="%s" data-meta="%s"><div class="ptitle"><h2>%s</h2><span>%s</span></div>%s%s</div>' % (
            key, esc(metas[key]), esc(name), esc(metas[key]), '<p class="sec-note">%s</p>' % note if note else "", body))

    dup = []
    if not an["clusters"]:
        dup.append('<p class="empty-block">None found</p>')
    for c in an["clusters"]:
        count = {}
        for n in c["members"]:
            for path in {f["path"] for f in by_num[n].get("files", [])}:
                count[path] = count.get(path, 0) + 1
        shared = sorted(p for p, k in count.items() if k >= 2 and p not in hot_set)
        chips = "".join('<span class="fchip">%s</span>' % esc(p) for p in shared[:8])
        if len(shared) > 8:
            chips += '<span class="fchip">+%d more</span>' % (len(shared) - 8)
        members = "".join(
            '<div class="row"><div class="lead">%s</div><div><div class="ttl">%s</div>'
            '<div class="sys"><span>by %s</span>%s</div></div></div>' % (
                avatar(by_num[n]["author"]), pr_line(by_num[n]), esc(by_num[n]["author"]), state_pills(by_num[n]))
            for n in c["members"])
        label, label_area = cluster_label([by_num[n] for n in c["members"]])
        dup.append('<div class="dup" style="%s"><div class="chead"><h3>%s</h3><span class="small">%s</span></div>%s%s'
                   '<p class="small">Why: %s</p></div>' % (
                       cat_style(label_area), esc(label), esc(", ".join("#%d" % n for n in c["members"])), members,
                       '<div class="chips-f">%s</div>' % chips if chips else "", esc(cluster_why(c))))
    ready = ['<p class="empty-block">None</p>'] if not done else [
        '<div class="row"><div class="lead"><b>%dd</b>old</div><div><div class="ttl">%s</div><div class="sys">%s%s'
        '<span><span class="add">+%d</span> <span class="del">&minus;%d</span> &middot; %s files</span></div></div></div>' % (
            p["ageDays"], pr_line(p), ptype(p["primary"]), ci_pill(p["ci"]), p["additions"], p["deletions"], esc(pr_files_note(p)))
        for p in done]
    hot = ['<p class="empty-block">None</p>'] if not an["hotFiles"] else [
        '<div class="row"><div class="lead"><b>%d</b>PRs</div><div><div class="mono">%s</div>'
        '<span class="heat"><i style="width:%d%%"></i></span><div class="sys">%s</div></div></div>' % (
            h["count"], esc(h["file"]), round(100 * h["count"] / max(x["count"] for x in an["hotFiles"])),
            " ".join(plink(n) for n in h["prs"][:12]) + (" +%d more" % (len(h["prs"]) - 12) if len(h["prs"]) > 12 else ""))
        for h in an["hotFiles"]]
    allp = []
    for a in areas:
        ps = sorted(an["areas"][a], key=lambda p: p["number"])
        rows = []
        for p in ps:
            also = [x for x in p["areas"] if x != p["primary"]]
            q = " ".join([str(p["number"]), "#%d" % p["number"], p["title"], p["author"]] + [f["path"] for f in p.get("files", [])]).lower()
            rows.append(
                '<div class="row pr" data-area="%s" data-draft="%d" data-unrev="%d" data-done="%d" data-q="%s"><div class="lead">%s</div>'
                '<div><div class="ttl">%s%s</div><div class="sys">%s<span>%dd</span>'
                '<span><span class="add">+%d</span> <span class="del">&minus;%d</span></span><span>%s files</span></div></div></div>' % (
                    esc(slug(a)), bool(p["isDraft"]), p["unreviewed"], p["looksDone"], esc(q), avatar(p["author"]),
                    pr_line(p), ' <span class="small">(also %s)</span>' % esc(", ".join(also)) if also else "",
                    state_pills(p), p["ageDays"], p["additions"], p["deletions"], esc(pr_files_note(p))))
        allp.append('<details class="area" id="%s"><summary>%s <span class="cnt">%d</span></summary>%s</details>' % (
            esc(slug(a)), ptype(a), len(ps), "".join(rows)))
    o.append(panel("dup", "".join(dup), "Linked by changed files (Jaccard &ge; %.1f) or title words (Jaccard &ge; %.1f). Hot files are ignored." % (FILE_JACCARD, TITLE_JACCARD)))
    o.append(panel("ready", "".join(ready), "Not draft, CI green, and unreviewed or approved."))
    o.append(panel("hot", "".join(hot), "Touched by more than %g open PRs: conflict hotspots, left out of the duplicate groups." % an["hotThreshold"]))
    o.append(panel("all", "".join(allp)))
    o.append("</div></section>")

    # ---- meta strip
    o.append('<section class="a-meta" aria-label="Overall stats">%s</section>' % "".join((
        stat("Open", n_open, dl("open", n_open), cls="key"),
        stat("Unreviewed", n_un, dl("unreviewed", n_un)),
        stat("Looks done", n_done, dl("looksDone", n_done)),
        stat("Possible duplicates", n_clusters, dl("clusters", n_clusters)),
        stat("Drafts", n_dr, dl("draft", n_dr)),
        stat("Stale (%dd+)" % STALE_DAYS, n_stale, show_delta=False),
        stat("CI failing", n_fail, show_delta=False, cls="warn" if n_fail else ""))))
    o.append('<footer class="foot">Generated %s UTC for %s. Read-only scan of public PR metadata.</footer></div>'
             '<div class="tip" id="tip" hidden></div><script>%s</script><script>%s</script></body></html>' % (
                 now.strftime("%Y-%m-%d %H:%M"), esc(repo), JS, SCENE_JS))
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
        return " (no 24 h data yet)" if d is None else " (%+d in 24 h)" % d

    head = ["| PR | Title | Author | Age | Review | CI | Files |", "|---|---|---|---|---|---|---|"]
    n_un, n_dr = sum(p["unreviewed"] for p in prs), sum(bool(p["isDraft"]) for p in prs)
    n_done, n_stale = sum(p["looksDone"] for p in prs), sum(p["stale"] for p in prs)
    o = ["# Open pull requests: %s" % repo, "", "Updated %s UTC." % now.strftime("%Y-%m-%d %H:%M"), ""]
    o.append("Open %d%s, unreviewed %d%s, looks done %d%s, possible duplicates (clusters) %d%s, drafts %d%s, stale %d." % (
        len(prs), ch("open", len(prs)), n_un, ch("unreviewed", n_un), n_done, ch("looksDone", n_done),
        len(an["clusters"]), ch("clusters", len(an["clusters"])), n_dr, ch("draft", n_dr), n_stale))
    recent = window(hist, now, TREND_DAYS)
    o += ["", "## Trends", ""]
    if len(recent) < 2:
        o += ["No data yet.", "", "Trends start after the first hours of hourly snapshots (%d so far)." % len(recent)]
    else:
        o += ["Last %d days, last snapshot of each UTC day; opened and closed from consecutive snapshots (- = no earlier snapshot to compare)." % TREND_DAYS, "",
              "| Day | Open | Unreviewed | Opened | Closed |", "|---|---|---|---|---|"]
        o += ["| %s | %d | %d | %s | %s |" % (d["day"], d["open"], d["unreviewed"], d["new"] if d["pairs"] else "-", d["closed"] if d["pairs"] else "-")
              for d in daily_summary(recent)]
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


def merged_last_24h(repo, now):
    """Number of PRs merged in the last 24 h, or None when the query fails (shown as "no data yet")."""
    try:
        since = now - dt.timedelta(hours=24)
        return sum(1 for p in fetch_merged(repo, now) if p.get("mergedAt") and parse_time(p["mergedAt"]) >= since)
    except (SystemExit, Exception) as e:
        print("pr-scan: merged-PR query failed: %s" % e, file=sys.stderr)
        return None


def cmd_report(args):
    now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    an = build_analysis(fetch_open(args.repo), now)
    an["merged24"] = merged_last_24h(args.repo, now)
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
