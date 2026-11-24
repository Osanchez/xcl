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


_LIGHT_CAT, _DARK_CAT = category_css()
_DARK_TOKENS = ("--page:#0e1219;--ink:#eef2f8;--soft:#a2adbc;--rule:#2a3240;--wash:#1a212c;--accent:#8ca8ff;--partial:#ffab5c;--partial-text:#ffab5c;"
                "--empty:#39424f;--tip:#eef2f8;--tip-text:#0e1219;--tip-soft:#3a4454;--good:#b4e06a;--bad:#ff8a80;" + _DARK_CAT)
CSS = (":root{color-scheme:light dark;--page:#fff;--ink:#10141c;--soft:#566070;--rule:#d9dee6;--wash:#eef1f5;--accent:#1546e0;"
       "--partial:#e8770c;--partial-text:#a94f00;--empty:#c9cfd9;--tip:#10141c;--tip-text:#fff;--tip-soft:#d7dce4;--good:#4f7f16;--bad:#c4261d;"
       + _LIGHT_CAT + "}\n"
       "@media (prefers-color-scheme:dark){:root:not([data-theme=light]){" + _DARK_TOKENS + "}}\n"
       ":root[data-theme=dark]{color-scheme:dark;" + _DARK_TOKENS + "}\n:root[data-theme=light]{color-scheme:light}\n") + """
*{box-sizing:border-box}
html{background:var(--page);-webkit-text-size-adjust:100%}
body{margin:0;background:var(--page);color:var(--ink);font:400 15px/1.45 Roboto,system-ui,-apple-system,"Segoe UI",Arial,sans-serif;font-variant-numeric:tabular-nums}
h1,h2,h3,p,ol,ul{margin:0;padding:0}ol,ul{list-style:none}
h1,h2,h3,.cond,.score,.stat b,.rank,.step b,th{font-family:"Roboto Condensed","Arial Narrow",Roboto,system-ui,sans-serif}
a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}
a:focus-visible,button:focus-visible,input:focus-visible,summary:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.app{display:grid;grid-template-columns:minmax(0,1fr);padding:0 16px;
grid-template-areas:"top" "city" "road" "standings" "filters" "feed" "meta" "foot"}
.app>*{min-width:0}
.top{grid-area:top;display:flex;align-items:center;justify-content:space-between;gap:14px;min-height:50px;border-bottom:2px solid var(--ink)}
h1{font:700 20px/1.1 "Roboto Condensed",Arial,sans-serif;text-transform:uppercase;letter-spacing:.04em}h1 span{color:var(--soft)}
h2{font-size:16px;line-height:1.2;font-weight:700}h3{font-size:15px;line-height:1.3;font-weight:700}
.top-right{display:flex;align-items:center;gap:12px;min-width:0}
.live{display:flex;align-items:center;gap:8px;color:var(--soft);font-size:13px;white-space:nowrap}
.dot{width:8px;height:8px;background:var(--accent);animation:beat 2s steps(2) infinite}
@keyframes beat{50%{opacity:.25}}
.theme{border:1px solid var(--rule);background:transparent;color:var(--ink);font:500 13px/1 Roboto,Arial,sans-serif;padding:7px 10px;cursor:pointer}
.theme:hover{border-color:var(--ink)}
.filters{grid-area:filters;display:none;flex-direction:column;gap:8px;padding:9px 0;border-top:1px solid var(--rule);border-bottom:1px solid var(--rule)}
.js .filters{display:flex}
.frow{display:flex;flex-wrap:wrap;align-items:center;gap:8px 16px}
.chips{display:flex;gap:6px;overflow-x:auto;scrollbar-width:thin;padding-bottom:2px}
.chip{flex:none;border:1px solid var(--rule);background:transparent;color:var(--ink);font:500 13px/1 Roboto,Arial,sans-serif;padding:7px 10px;cursor:pointer;border-radius:0}
.chip:hover{border-color:var(--ink)}
.chip[aria-pressed=true]{background:var(--ink);border-color:var(--ink);color:var(--page)}
.filters input{font:500 13px/1.2 Roboto,Arial,sans-serif;padding:7px 10px;border:1px solid var(--rule);background:var(--page);color:var(--ink);min-width:0;flex:1 1 240px;max-width:380px;border-radius:0}
.result{font-size:13px;color:var(--soft)}
.a-city,.a-road,.a-standings,.a-feed{min-width:0;min-height:0;padding:12px 0}
.a-road,.a-standings,.a-feed{border-top:1px solid var(--rule)}
.a-city{grid-area:city}.a-road{grid-area:road;display:flex;flex-direction:column}.a-standings{grid-area:standings;display:flex;flex-direction:column}
.a-feed{grid-area:feed;display:flex;flex-direction:column}
.head{display:flex;justify-content:space-between;align-items:baseline;gap:10px;margin-bottom:8px;min-width:0}
.head>span{color:var(--soft);font-size:13px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.score-box{display:flex;align-items:flex-end;flex-wrap:wrap;gap:8px 16px;margin-bottom:8px}
.score{font:700 72px/.86 "Roboto Condensed",Arial,sans-serif}.score small{font-size:28px;margin-left:6px;color:var(--soft)}
.score-note{color:var(--soft);font-size:13px;line-height:1.35}.score-note b{display:block;color:var(--ink);font-size:15px;font-weight:500}
.score-note span{display:block}
.sec-note{color:var(--soft);font-size:13px;line-height:1.35;margin:4px 0 8px}
.chart-block{padding-top:12px;border-top:1px solid var(--rule);margin-top:12px}
.chart-block h3{font-weight:500;margin-bottom:4px}
.chart{display:block;width:100%;height:auto;margin-top:4px}
.chart .grid{stroke:var(--rule);stroke-width:1}.chart .tick{fill:var(--soft);font-size:12px;font-family:Roboto,Arial,sans-serif}
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
.st-axis,.st{display:grid;grid-template-columns:22px minmax(128px,1fr) minmax(40px,72px) 56px;gap:0 8px;align-items:center}
.st-axis{position:sticky;top:0;z-index:1;padding:0 4px 4px 2px;background:var(--page);color:var(--soft);font-size:13px;line-height:1.2}
.st-axis span:last-child{text-align:right}
.st-list{overflow-y:auto;min-height:0;flex:1;max-height:420px}
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
.spark{display:block;width:100%;height:20px}.spline{vector-effect:non-scaling-stroke;fill:none;stroke:var(--soft);stroke-width:1.5;stroke-linejoin:round}.nodata{font-size:12px;color:var(--soft)}
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
.a-meta{grid-area:meta;display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px 16px;padding:12px 0;border-top:1px solid var(--rule)}
.stat{min-width:0}.stat span{display:block;color:var(--soft);font-size:13px;line-height:1.25}
.stat b{display:block;font-size:20px;line-height:1.15;white-space:nowrap}.stat.key b{color:var(--accent)}.stat.warn b{color:var(--partial-text)}
.stat small{display:block;color:var(--soft);font-size:13px;line-height:1.25}
.foot{grid-area:foot;padding:8px 0 10px;border-top:1px solid var(--rule);color:var(--soft);font-size:13px;line-height:1.35}
.small{color:var(--soft);font-size:13px;line-height:1.35}
.tip{position:fixed;top:0;left:0;z-index:30;max-width:260px;padding:6px 9px;background:var(--tip);color:var(--tip-text);font-size:13px;line-height:1.3;pointer-events:none}
.tip[hidden]{display:none}
.vh{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
[hidden]{display:none!important}
@media (min-width:640px){.a-meta{grid-template-columns:repeat(4,minmax(0,1fr))}}
@media (max-width:760px){.road{grid-template-columns:1fr}.step{display:grid;grid-template-columns:44px minmax(0,1fr) auto;align-items:center;gap:8px;padding:6px 8px;border-bottom-width:2px}.step .pips{display:none}}
@media (max-width:560px){.st{grid-template-columns:22px minmax(0,1fr) 56px}.st .track{grid-column:2/4;grid-row:2}.st-axis{display:none}.score{font-size:56px}}
@media (max-width:620px){.top{flex-wrap:wrap;gap:4px 12px;padding:7px 0}.top-right{width:100%;justify-content:space-between}}
@media (min-width:1100px) and (min-height:660px){
.app{height:100dvh;overflow:hidden;column-gap:24px;padding:0 20px;
grid-template-columns:minmax(0,1fr) minmax(0,1fr) minmax(0,1.15fr);grid-template-rows:auto auto minmax(0,1fr) 172px auto auto;
grid-template-areas:"top top top" "filters filters filters" "city standings feed" "road standings feed" "meta meta meta" "foot foot foot"}
.a-city,.a-standings,.a-feed{overflow:auto}.a-city{padding-top:12px}.a-road{overflow:hidden}
.st-list,.feed-body{max-height:none}.feed-body{overflow:auto}.a-feed,.a-standings{overflow:hidden}
.a-meta{display:flex;justify-content:space-between;gap:18px}.stat{flex:none}
.foot{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
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
q=d.getElementById('q'),res=d.getElementById('result'),btns=F?[].slice.call(F.querySelectorAll('button')):[];
var st={q:'',area:'',draft:0,unrev:0,done:0};
function dec(s){try{return decodeURIComponent(s.replace(/\\+/g,' '))}catch(e){return ''}}
function parse(){st={q:'',area:'',draft:0,unrev:0,done:0};var tt='';
(location.hash||'').replace(/^#/,'').split('&').forEach(function(kv){var i=kv.indexOf('=');if(i<1)return;
var k=kv.slice(0,i),v=dec(kv.slice(i+1));if(k==='q'||k==='area')st[k]=v;else if(k==='tab')tt=v;else if(k in st)st[k]=v==='1'?1:0;});return tt;}
function on(){return !!(st.q.trim()||st.area||st.draft||st.unrev||st.done);}
function write(){var p=[];if(st.q)p.push('q='+encodeURIComponent(st.q));if(st.area)p.push('area='+encodeURIComponent(st.area));
['draft','unrev','done'].forEach(function(k){if(st[k])p.push(k+'=1')});if(tab!=='dup')p.push('tab='+tab);
history.replaceState(null,'',p.length?'#'+p.join('&'):location.pathname+location.search);}
function apply(){var t=st.q.toLowerCase().trim(),f=on(),shown=0;
rows.forEach(function(r){var ok=(!t||r.getAttribute('data-q').indexOf(t)>=0)&&(!st.area||r.getAttribute('data-area')===st.area)&&
(!st.draft||r.getAttribute('data-draft')==='1')&&(!st.unrev||r.getAttribute('data-unrev')==='1')&&(!st.done||r.getAttribute('data-done')==='1');
r.hidden=!ok;if(ok)shown++;});
groups.forEach(function(g){var n=g.querySelectorAll('.pr:not([hidden])').length,c=g.querySelector('.cnt'),tot=g.querySelectorAll('.pr').length;
g.hidden=n===0;if(f)g.open=n>0;if(c)c.textContent=f?n+' of '+tot:String(tot);});
btns.forEach(function(b){var k=b.getAttribute('data-flag'),ar=b.getAttribute('data-area');
if(k||ar!==null)b.setAttribute('aria-pressed',String(k?!!st[k]:ar===st.area));});
if(res)res.textContent=f?'Showing '+shown+' of '+rows.length+' pull requests':rows.length+' pull requests';
var allp=d.querySelector('.panel[data-panel=all]');if(allp&&f)allp.setAttribute('data-meta','Showing '+shown+' of '+rows.length);
else if(allp)allp.setAttribute('data-meta',rows.length+' pull requests');
showTab(tab);}
function sync(){if(q)q.value=st.q;apply();}
if(F){q.addEventListener('input',function(){st.q=q.value;if(on())tab='all';write();apply();});
F.addEventListener('click',function(e){var b=e.target.closest('button');if(!b)return;var k=b.getAttribute('data-flag'),ar=b.getAttribute('data-area');
if(k)st[k]=st[k]?0:1;else st.area=st.area===ar?'':ar;if(on())tab='all';write();apply();});}
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


def render_html(an, repo, now, hist=None):
    hist = hist or []
    prs = an["prs"]
    by_num = {p["number"]: p for p in prs}
    hot_set = {h["file"] for h in an["hotFiles"]}
    areas = sorted(an["areas"], key=lambda a: (-len(an["areas"][a]), a))
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
    o.append("<!doctype html><html lang=en><head><meta charset=utf-8>"
             "<meta name=viewport content='width=device-width,initial-scale=1'>"
             "<title>PR scan: %s</title>"
             "<link rel=preconnect href='https://fonts.googleapis.com'><link rel=preconnect href='https://fonts.gstatic.com' crossorigin>"
             "<link rel=stylesheet href='%s'><script>%s</script><style>%s</style></head><body><div class=\"app\">" % (
                 esc(repo), FONTS_URL, THEME_JS, CSS))
    o.append('<header class="top"><h1>PR scan <span>/ <a href="https://github.com/%s">%s</a></span></h1>'
             '<div class="top-right"><div class="live"><i class="dot" aria-hidden="true"></i>'
             '<span><span id="ago">Updated </span><time id="upd" datetime="%s">%s UTC</time></span></div>'
             '<button type="button" class="theme" id="theme" aria-label="Colour theme">Theme</button></div></header>' % (
                 esc(repo), esc(repo), now.strftime(TS_FMT), now.strftime("%Y-%m-%d %H:%M")))

    # ---- city panel: score box + trends
    d_open = change_24h(hist, "open", n_open, now)
    o.append('<section class="a-city" aria-label="Summary and trends"><div class="score-box"><div class="score">%d<small>open</small></div>'
             '<p class="score-note"><b>%d unreviewed &middot; %d look done</b><span>%d possible duplicate group%s &middot; %d draft%s</span>%s</p></div>' % (
                 n_open, n_un, n_done, n_clusters, "" if n_clusters == 1 else "s", n_dr, "" if n_dr == 1 else "s", delta_text(d_open)))
    recent = window(hist, now, TREND_DAYS)
    o.append('<div class="head" style="margin-top:16px"><h2>Trends</h2><span>last %d days, %d snapshot%s</span></div>' % (
        TREND_DAYS, len(recent), "" if len(recent) == 1 else "s"))
    if len(recent) < 2:
        o.append('<p class="empty-block">No data yet</p><p class="sec-note">Trends start after the first hours of hourly snapshots (%d so far).</p>' % len(recent))
    else:
        days = [x for x in daily_summary(recent) if x["pairs"]] if len({x["ts"][:10] for x in recent}) >= 2 else []
        o.append('<div class="chart-block"><h3>Open and unreviewed</h3>%s%s</div>' % (
            legend([("Open", "s-open"), ("Unreviewed", "s-unrev")]),
            svg_line_chart([
                {"name": "open", "cls": "s-open", "points": [(parse_time(x["ts"]), x["open"]) for x in recent]},
                {"name": "unreviewed", "cls": "s-unrev", "points": [(parse_time(x["ts"]), x["unreviewed"]) for x in recent]}],
                "Open and unreviewed pull requests over the last %d days" % TREND_DAYS)))
        o.append('<div class="chart-block"><h3>Opened and closed per day</h3>%s%s</div>' % (
            legend([("Opened", "b-new"), ("Closed or merged", "b-closed")]),
            svg_bars(days, "Pull requests opened and closed per day (UTC)")))
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
    biggest = max([len(ps) for ps in an["areas"].values()] or [1])
    o.append('<section class="a-standings" aria-label="Areas"><div class="head"><h2>By area</h2><span>ranked by open PRs, line = %dd</span></div>'
             '<div style="margin-bottom:8px">%s</div><div class="st-list"><div class="st-axis" aria-hidden="true"><span>#</span><span>Area</span>'
             '<span>State mix</span><span>Open</span></div><ol>' % (SPARK_DAYS, legend([(n, "k-" + k) for k, n in BUCKETS])))
    for i, a in enumerate(areas, 1):
        ps = an["areas"][a]
        cnt = {k: sum(bucket(p) == k for p in ps) for k, _ in BUCKETS}
        segs = "".join('<i class="%s" style="flex:%d" data-tip="%s" title="%s"></i>' % (
            k, cnt[k], esc("%s: %d" % (n, cnt[k])), esc("%s: %d" % (n, cnt[k]))) for k, n in BUCKETS if cnt[k])
        series = downsample([s["areas"].get(a, 0) for s in spark_hist if s["open"] == 0 or s["areas"]], 48)
        o.append('<li class="st"><span class="rank">%d</span><div class="who"><b>%s</b><small>%s</small></div>'
                 '<span class="track"><span class="fill" style="width:%.1f%%">%s</span></span>'
                 '<div class="where"><b>%d</b>%s</div></li>' % (
                     i, ptype(a, link=True),
                     esc(" · ".join("%d %s" % (cnt[k], n.lower()) for k, n in BUCKETS if cnt[k])),
                     100.0 * len(ps) / biggest, segs, len(ps),
                     sparkline(series, "%s: open PRs over %d days" % (a, SPARK_DAYS))))
    o.append("</ol></div></section>")

    # ---- filters
    o.append('<div class="filters" id="filters"><div class="frow"><label class="vh" for="q">Search pull requests</label>'
             '<input id="q" type="search" placeholder="Search title, number, author or path" autocomplete="off">%s'
             '<span class="result" id="result" role="status"></span></div><div class="chips" role="group" aria-label="Area filters">%s</div></div>' % (
                 "".join('<button type="button" class="chip" data-flag="%s" aria-pressed="false">%s</button>' % (k, n)
                         for k, n in (("draft", "Draft"), ("unrev", "Unreviewed"), ("done", "Looks done"))),
                 "".join('<button type="button" class="chip" data-area="%s" aria-pressed="false">%s</button>' % (
                     esc(slug(a)), esc(a)) for a in areas)))

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
             '<div class="tip" id="tip" hidden></div><script>%s</script></body></html>' % (now.strftime("%Y-%m-%d %H:%M"), esc(repo), JS))
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
