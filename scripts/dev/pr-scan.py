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
CLOSED_DAYS = 30         # PRs closed (merged or not) in this window are fetched (paged); the time range picks what is shown
FILE_JACCARD = 0.5
TITLE_JACCARD = 0.6
CLUSTER_MAX = 12         # a larger connected component is re-split with stricter thresholds
JACCARD_STEP, JACCARD_CAP = 0.1, 0.9
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
# Areas shown together as one tower / standings row; a PR's group comes from its primary area.
AREA_GROUPS = [("Core runtime", ("kernelx", "xcl run", "Runtime", "VFS", "TLS", "PE loader", "Memory")),
               ("Tooling", ("Scripts", "CI", "README", "Docs"))]
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


def group_of(area):
    """The group an area belongs to, or the area itself when it is ungrouped."""
    for name, members in AREA_GROUPS:
        if area in members:
            return name
    return area


def area_label(area):
    return area


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


def link_reason(a, b, hot=frozenset(), fj=FILE_JACCARD, tj=TITLE_JACCARD):
    """Why two PRs are linked, or None. Dicts need 'file_set' (set of paths) and 'title'.
    Hot files are left out of the file Jaccard, and a file link needs a shared non-hot file."""
    fa, fb = a["file_set"] - hot, b["file_set"] - hot
    fs = jaccard(fa, fb) if fa & fb else 0.0
    if fs >= fj:
        return {"kind": "shared files", "score": round(fs, 2)}
    ts = jaccard(title_tokens(a["title"]), title_tokens(b["title"]))
    if ts >= tj:
        return {"kind": "similar title", "score": round(ts, 2)}
    return None


def _components(items, idx, hot, fj, tj):
    """[(member indexes, [(i, j, reason)])] of the connected components (size >= 2) of the link graph over items[idx]."""
    parent = {i: i for i in idx}

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    links = []
    for x, i in enumerate(idx):
        for j in idx[x + 1:]:
            r = link_reason(items[i], items[j], hot, fj, tj)
            if r:
                links.append((i, j, r))
                parent[find(i)] = find(j)
    groups = {}
    for i in idx:
        groups.setdefault(find(i), []).append(i)
    return [(g, [l for l in links if find(l[0]) == root]) for root, g in groups.items() if len(g) >= 2]


def _split(items, idx, hot, fj, tj, max_size):
    out = []
    for g, links in _components(items, idx, hot, fj, tj):
        if len(g) > max_size and (fj < JACCARD_CAP or tj < JACCARD_CAP):
            out += _split(items, g, hot, min(JACCARD_CAP, round(fj + JACCARD_STEP, 2)),
                          min(JACCARD_CAP, round(tj + JACCARD_STEP, 2)), max_size)
        else:
            out.append((g, links))
    return out


def clusters(prs, hot=frozenset(), max_size=CLUSTER_MAX):
    """Connected components (size >= 2) of the link graph, with the links that explain them.

    A component with more than max_size members is re-split inside itself by raising the file and title Jaccard
    thresholds by JACCARD_STEP per step (up to JACCARD_CAP) until every part has max_size members or fewer or the cap
    is reached; parts left with one member are dropped.
    prs: dicts with number, title, files (list of {path,...}). Returns a list of
    {"members": [numbers], "links": [{"a","b","kind","score"}]}, largest first."""
    items = []
    for p in prs:
        q = dict(p)
        q["file_set"] = frozenset(f["path"] for f in p.get("files", []))
        items.append(q)
    out = []
    for g, links in _split(items, list(range(len(items))), hot, FILE_JACCARD, TITLE_JACCARD, max_size):
        out.append({
            "members": sorted(items[i]["number"] for i in g),
            "links": [{"a": items[i]["number"], "b": items[j]["number"], **r} for i, j, r in links],
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


def closed_time(pr):
    """When a closed or merged PR ended: mergedAt, else closedAt (None when neither is known)."""
    return pr.get("mergedAt") or pr.get("closedAt")


def closed_window(prs, now, days=CLOSED_DAYS):
    """The closed or merged PRs that ended in the last `days` days, newest first (ties by number). Open PRs and PRs
    without an end time are left out."""
    since = now - dt.timedelta(days=days)
    out = [p for p in prs if p.get("state") in ("MERGED", "CLOSED") and closed_time(p) and parse_time(closed_time(p)) >= since]
    return sorted(out, key=lambda p: (-parse_time(closed_time(p)).timestamp(), p["number"]))


def enrich_closed(pr, now):
    out = enrich(pr, now)
    out["merged"] = bool(pr.get("mergedAt"))
    out["closedAgeDays"] = age_days(closed_time(pr), now)
    return out


# ---- time ranges: the page shows one dataset per range, all computed here (the JS only swaps them)

RANGES = [("1h", 3600, "1 h"), ("12h", 12 * 3600, "12 h"), ("24h", 24 * 3600, "24 h"), ("7d", 7 * 86400, "7 d"), ("30d", 30 * 86400, "30 d")]
DEFAULT_RANGE = "7d"
RANGE_LABEL = {k: lb for k, _, lb in RANGES}
DEFAULT_RL = RANGE_LABEL[DEFAULT_RANGE]


def empty_range(rl):
    return "No PRs in the last %s." % rl


def in_range(pr, range_s, now):
    """Closed or merged PR: it ended (mergedAt, else closedAt) within range_s seconds before now. Open PR: it was created or
    updated within range_s seconds. The boundary is inclusive."""
    since = now - dt.timedelta(seconds=range_s)
    if pr.get("state", "OPEN") == "OPEN":
        return any(pr.get(k) and parse_time(pr[k]) >= since for k in ("createdAt", "updatedAt"))
    t = closed_time(pr)
    return bool(t) and parse_time(t) >= since


def range_view(an, range_s, now):
    """The analysis restricted to one range: open PRs active in it (areas regrouped) and closed PRs that ended in it
    (None stays None). Shaped like build_analysis' result for top_areas and effort_scene."""
    prs = [p for p in an["prs"] if in_range(p, range_s, now)]
    areas = {}
    for p in prs:
        areas.setdefault(p["primary"], []).append(p)
    cl = None if an.get("closed") is None else [p for p in an["closed"] if in_range(p, range_s, now)]
    return {"prs": prs, "areas": areas, "closed": cl, "mergedPrs": None if cl is None else [p for p in cl if p.get("merged")]}


def visible_clusters(an, range_s, now):
    """The duplicate groups as they look in a range: members outside it are dropped; a group needs two members left and an
    open one. merged / superseded follow the members left."""
    by_num = {p["number"]: p for p in an["prs"] + (an.get("closed") or [])}
    out = []
    for c in an["clusters"]:
        ms = [n for n in c["members"] if n in by_num and in_range(by_num[n], range_s, now)]
        if len(ms) >= 2 and any(by_num[n].get("state", "OPEN") == "OPEN" for n in ms):
            merged = [n for n in c.get("merged", []) if n in ms]
            out.append(dict(c, members=ms, merged=merged, superseded=bool(merged)))
    return out


def hot_in_range(an, range_s, now):
    """Hot-file rows that keep at least one PR in the range, as [(file entry, [PR numbers in range])]."""
    by_num = {p["number"]: p for p in an["prs"]}
    out = []
    for h in an["hotFiles"]:
        ns = [n for n in h["prs"] if n in by_num and in_range(by_num[n], range_s, now)]
        if ns:
            out.append((h, ns))
    return out


def range_counts(an, range_s, now):
    """Counts of one range, for the headline, the stats and the tab labels: active (open PRs created or updated in it),
    merged and closed (without a merge) in it, plus unreviewed / done / draft among the active ones, and the visible rows
    of the Duplicates, Hot files lists."""
    v = range_view(an, range_s, now)
    cl = v["closed"]
    return {"active": len(v["prs"]), "merged": None if cl is None else len(v["mergedPrs"]),
            "closed": None if cl is None else len(cl) - len(v["mergedPrs"]),
            "unreviewed": sum(bool(p["unreviewed"]) for p in v["prs"]), "done": sum(bool(p["looksDone"]) for p in v["prs"]),
            "draft": sum(bool(p["isDraft"]) for p in v["prs"]),
            "dup": len(visible_clusters(an, range_s, now)), "hot": len(hot_in_range(an, range_s, now))}


def range_metas(c, rl):
    """The tab meta texts of a range from its counts."""
    return {"dup": "%d group%s" % (c["dup"], "" if c["dup"] == 1 else "s"), "ready": "%d, oldest first" % c["done"],
            "hot": "%d file%s" % (c["hot"], "" if c["hot"] == 1 else "s"), "all": "%d pull requests" % c["active"],
            "closed": "%d merged, %d closed, last %s" % (c["merged"], c["closed"], rl) if c["merged"] is not None else "no data yet"}


def mark_superseded(cls, by_num):
    """Clusters over open and closed PRs, for display: a cluster with no open member is dropped; one with a merged member
    gets "merged" (those numbers, sorted) and "superseded": True. by_num: enriched PRs (closed ones carry "merged")."""
    out = []
    for c in cls:
        ms = [by_num[n] for n in c["members"]]
        if not any(p.get("state", "OPEN") == "OPEN" for p in ms):
            continue
        merged = sorted(p["number"] for p in ms if p.get("merged"))
        out.append(dict(c, merged=merged, superseded=bool(merged)))
    return out


def build_analysis(prs, now, closed=None):
    """closed: normalised PRs closed recently (merged or not), or None when they could not be fetched."""
    prs = [enrich(p, now) for p in prs]
    per_area = {}
    for p in prs:
        per_area.setdefault(p["primary"], []).append(p)
    hot = hot_files(prs)
    hot_set = frozenset(hot)
    cl = None if closed is None else [enrich_closed(p, now) for p in closed_window(closed, now)]
    everyone = prs + (cl or [])
    hot_all = frozenset(hot_files(everyone))  # clustering runs over open and closed PRs, so hot files do too
    area_clusters = {a: clusters(ps, hot_set) for a, ps in per_area.items() if len(ps) > 1}
    area_clusters = {a: c for a, c in area_clusters.items() if c}
    return {
        "prs": prs,
        "areas": per_area,
        "closed": cl,
        "mergedPrs": None if cl is None else [p for p in cl if p["merged"]],
        "clusters": mark_superseded(clusters(everyone, hot_all), {p["number"]: p for p in everyone}),
        "clusterHot": sorted(hot_all),
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


def closed_tag(p):
    return pill("merged", "merged") if p.get("merged") else pill("closed")


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


OPENED_CLOSED = (("new", "b-new", "opened"), ("closed", "b-closed", "closed"))


def svg_bars(rows, label, w=480, h=240, series=OPENED_CLOSED):
    """rows: [{"day", <series keys>}], grouped bars per day; series: [(row key, css class, name)]."""
    if not rows:
        return '<p class="muted empty">No data yet.</p>'
    ml, mr, mt, mb = 36, 12, 12, 28
    step, top = nice_ticks(max(max(r[k] for k, _, _ in series) for r in rows))
    pw = w - ml - mr
    gw = pw / max(len(rows), 4)
    bw = min(14, gw * 0.36)
    body = [_y_axis(ml, mr, mt, mb, w, h, top, step)]
    every = 1 if len(rows) <= 8 else 2
    for i, r in enumerate(rows):
        cx = ml + gw * (i + 0.5)
        for j, (key, cls, name) in enumerate(series):
            bh = (h - mt - mb) * r[key] / top
            bx = cx - (len(series) * bw + (len(series) - 1) * 2) / 2 + j * (bw + 2)
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


FONTS_URL = "https://fonts.googleapis.com/css2?family=Anton&family=Roboto:wght@400;500;700&display=swap"

# Area colour pairs (light, dark); areas past the last pair cycle.
CATEGORY = [("#d81f82", "#ff7fc0"), ("#c96a12", "#ffb057"), ("#14839f", "#5fd0ea"), ("#4f7f16", "#b4e06a"),
            ("#7b6a78", "#cdb9c8"), ("#6a4cc7", "#b8a6ff"), ("#a8801a", "#e8cb62"), ("#c43b3b", "#ff9c94"),
            ("#2d7d5b", "#7fd9ae"), ("#8a4fb0", "#dcaef2")]


def category_css():
    light = "".join("--cat%d:%s;" % (i, c[0]) for i, c in enumerate(CATEGORY))
    dark = "".join("--cat%d:%s;" % (i, c[1]) for i, c in enumerate(CATEGORY))
    return light, dark


# Page tokens (light, dark). Light is a plain white page with flat white panels and rule lines. Dark sits on the fixed
# sunset gradient (SUNSET_DARK, top to bottom) with a translucent surface for panels, top bar and callouts. The scene
# itself has no backdrop: the canvas is transparent.
SUNSET_DARK = ("#ff7a1a", "#c2185b", "#4a148c", "#120b2e")
GRADIENT_TOKENS = [("sky", "none", "linear-gradient(180deg," + ",".join(SUNSET_DARK) + ")"),
                   ("surface", "#fff", "rgba(14,18,25,.7)"),
                   ("edge", "#d9dee6", "transparent")]
_LIGHT_SCENE = "".join("--%s:%s;" % (n, a) for n, a, _ in GRADIENT_TOKENS)
_DARK_SCENE = "".join("--%s:%s;" % (n, b) for n, _, b in GRADIENT_TOKENS)
_LIGHT_CAT, _DARK_CAT = category_css()
_DARK_TOKENS = ("--page:#0e1219;--ink:#eef2f8;--soft:#a2adbc;--rule:#2a3240;--wash:#1a212c;--accent:#8ca8ff;--partial:#ffab5c;--partial-text:#ffab5c;"
                "--empty:#39424f;--tip:#eef2f8;--tip-text:#0e1219;--tip-soft:#3a4454;--good:#b4e06a;--bad:#ff8a80;--merged:#c9a3ff;" + _DARK_SCENE + _DARK_CAT)
# spacing tokens: an 8 px rhythm (4 8 12 16 24) plus the derived 10 / 14 / 20 px paddings the minimums need
SPACING_TOKENS = ("--sp-1:4px;--sp-2:8px;--sp-3:12px;--sp-4:16px;--sp-5:24px;"
                  "--row-y:calc(var(--sp-3) - 2px);--row-x:var(--sp-3);--btn-x:calc(var(--sp-4) - 2px);--pan-x:calc(var(--sp-4) + var(--sp-1));"
                  "--pill-gap:calc(var(--sp-2) - 2px);")
CSS = (":root{color-scheme:light dark;--page:#fff;--ink:#10141c;--soft:#566070;--rule:#d9dee6;--wash:#eef1f5;--accent:#1546e0;"
       "--partial:#e8770c;--partial-text:#a94f00;--merged:#7a45d6;--empty:#c9cfd9;--tip:#10141c;--tip-text:#fff;--tip-soft:#d7dce4;--good:#456b12;--bad:#c4261d;"
       + SPACING_TOKENS
       + _LIGHT_SCENE + _LIGHT_CAT + "}\n"
       "@media (prefers-color-scheme:dark){:root:not([data-theme=light]){" + _DARK_TOKENS + "}}\n"
       ":root[data-theme=dark]{color-scheme:dark;" + _DARK_TOKENS + "}\n:root[data-theme=light]{color-scheme:light}\n") + """
*{box-sizing:border-box}
html{background:var(--page);-webkit-text-size-adjust:100%}
body::before{content:"";position:fixed;top:0;right:0;bottom:0;left:0;z-index:-1;pointer-events:none;background:var(--sky)}
body{margin:0;color:var(--ink);font:400 15px/1.45 Roboto,system-ui,-apple-system,"Segoe UI",Arial,sans-serif;font-variant-numeric:tabular-nums}
h1,h2,h3,p,ol,ul{margin:0;padding:0}ol,ul{list-style:none}
h1,h2,h3,.cond,.score,.stat b,.rank,.step b,th{font-family:Anton,Impact,"Arial Narrow",sans-serif;font-synthesis:none}
a{color:var(--accent);text-decoration:none}a:hover{text-decoration:underline}
a:focus-visible,button:focus-visible,input:focus-visible,summary:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.app{display:grid;grid-template-columns:minmax(0,1fr);padding:0 var(--sp-4);row-gap:var(--sp-2);
grid-template-areas:"top" "pages" "feed" "foot"}
.app>*{min-width:0}
.top{grid-area:top;display:flex;flex-direction:column;align-items:stretch;gap:var(--sp-3);padding:var(--sp-3) var(--sp-4);background:var(--surface);border-bottom:2px solid var(--ink)}
.nav-row{display:flex;flex-wrap:wrap;align-items:center;gap:var(--pill-gap) var(--sp-5);min-width:0}
h1{font:400 20px/1.1 Anton,Impact,sans-serif;text-transform:uppercase;letter-spacing:.04em}h1 span{color:var(--soft)}
h2{font-size:16px;line-height:1.2;font-weight:700}h3{font-size:15px;line-height:1.3;font-weight:700}
h1,h2,h3,.score,.stat b,.rank,.step b{font-weight:400}h2,h3{letter-spacing:.03em}
.top-right{display:flex;align-items:center;gap:var(--btn-x);min-width:0;margin-left:auto}
.live{display:flex;align-items:center;gap:var(--sp-2);color:var(--soft);font-size:13px;white-space:nowrap}
.dot{width:8px;height:8px;background:var(--accent);animation:beat 2s steps(2) infinite}
@keyframes beat{50%{opacity:.25}}
.theme{border:1px solid var(--rule);background:transparent;color:var(--ink);font:500 13px/1 Roboto,Arial,sans-serif;padding:var(--sp-2) var(--btn-x);cursor:pointer}
.theme:hover{border-color:var(--ink)}
.toolbar{display:none;align-items:center;flex-wrap:wrap;gap:var(--sp-2) var(--sp-4);min-width:0}
.js .toolbar{display:flex}
.toolbar input,.toolbar select{font:500 13px/1.2 Roboto,Arial,sans-serif;padding:var(--sp-2) var(--btn-x);border:1px solid var(--rule);background:var(--page);color:var(--ink);min-width:0;border-radius:0}
.toolbar input{flex:1 1 140px;max-width:340px}.toolbar select{flex:0 1 190px;max-width:100%;text-overflow:ellipsis}
.seg{display:inline-flex;max-width:100%;border:1px solid var(--rule)}
.seg button{flex:none;border:0;border-right:1px solid var(--rule);background:transparent;color:var(--ink);font:500 13px/1 Roboto,Arial,sans-serif;padding:var(--sp-2) var(--btn-x);cursor:pointer;border-radius:0}
.seg button:last-child{border-right:0}.seg button:hover{background:var(--wash)}
.seg button[aria-pressed=true]{background:var(--ink);color:var(--page)}
.result{font-size:13px;color:var(--soft);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0}.result:empty{display:none}
.a-city,.a-trends,.a-road,.a-standings,.a-feed{min-width:0;min-height:0}
.a-city{padding:var(--sp-2) var(--sp-3)}
.a-trends,.a-road,.a-standings,.a-feed{padding:var(--sp-4) var(--pan-x)}
.a-trends,.a-road,.a-standings,.a-feed,.a-meta,.foot{background:var(--surface);border:1px solid var(--edge)}
.a-city .score-box,.scene-foot,.scene-key,.a-city>.empty-block,.a-city>.sec-note{background:var(--surface);padding:var(--sp-3) var(--sp-4)}
.pages{grid-area:pages;display:flex;flex-direction:column;gap:var(--sp-2);min-width:0;min-height:0}
.page{display:flex;flex-direction:column;gap:var(--sp-2);min-width:0;min-height:0}
.page-title{font:400 16px/1.2 Anton,Impact,sans-serif;text-transform:uppercase;letter-spacing:.04em;color:var(--soft);padding-top:var(--sp-2)}.js .page-title{display:none}
.a-trends,.a-road,.a-standings{display:flex;flex-direction:column}
.a-feed{grid-area:feed;display:flex;flex-direction:column}
.ptabs{display:none;gap:var(--sp-1);min-width:0}.js .ptabs{display:flex}
.ptabs button{padding:var(--sp-2) var(--sp-3);border:0;border-bottom:2px solid transparent;background:none;color:var(--soft);font:400 15px/1.2 Anton,Impact,sans-serif;text-transform:uppercase;letter-spacing:.04em;cursor:pointer;border-radius:0}
.ptabs button:hover{color:var(--ink)}.ptabs button[aria-pressed=true]{border-bottom-color:var(--partial);color:var(--ink)}
.head{display:flex;justify-content:space-between;align-items:baseline;gap:var(--row-y);margin-bottom:var(--sp-2);min-width:0}
.head>span{color:var(--soft);font-size:13px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.score-box{display:flex;align-items:flex-end;flex-wrap:wrap;gap:var(--sp-2) var(--sp-4);margin-bottom:var(--sp-2)}
.score{font:400 72px/.86 Anton,Impact,sans-serif}.score small{font-size:28px;margin-left:6px;color:var(--soft)}
.score-note{color:var(--soft);font-size:13px;line-height:1.35}.score-note b{display:block;color:var(--ink);font-size:15px;font-weight:500}
.score-note span{display:block}
.sec-note{color:var(--soft);font-size:13px;line-height:1.35;margin:var(--sp-1) 0 var(--sp-2)}
.charts{display:grid;grid-template-columns:minmax(0,1fr);gap:var(--sp-3) var(--sp-4);flex:1;min-height:0}
.chart-block{display:flex;flex-direction:column;min-width:0;min-height:0}.chart-block.wide,.chart-block:only-child{grid-column:1/-1}
.chart-head{display:flex;flex-wrap:wrap;align-items:baseline;gap:0 var(--sp-3)}.chart-block h3{font-weight:500}
.chart-box{position:relative;flex:1;min-height:0}
.chart{display:block;width:100%;height:auto;margin-top:var(--sp-1)}
.chart .grid{stroke:var(--rule);stroke-width:1}.chart .tick{fill:var(--soft);font-size:13px;font-family:Roboto,Arial,sans-serif}
.chart .line{fill:none;stroke-width:2;stroke-linejoin:round}
.line.s-open{stroke:var(--accent)}.dot.s-open{fill:var(--accent)}.line.s-unrev{stroke:var(--partial)}.dot.s-unrev{fill:var(--partial)}
.chart .dot{stroke:none}.b-new{fill:var(--accent)}.b-closed{fill:var(--good)}.b-merged{fill:var(--merged)}
.legend{display:flex;flex-wrap:wrap;gap:var(--sp-1) var(--sp-4);font-size:13px;line-height:1.3;color:var(--soft)}
.legend li{display:flex;align-items:center;gap:var(--pill-gap)}.sw{display:inline-block;width:8px;height:8px}
.sw.s-open,.sw.b-new,.sw.k-rev{background:var(--accent)}.sw.s-unrev,.sw.k-wait{background:var(--partial)}.sw.b-closed,.sw.k-done{background:var(--good)}.sw.b-merged{background:var(--merged)}.sw.k-draft{background:var(--soft)}.sw.k-merged{background:var(--merged)}
.empty{margin:var(--sp-2) 0 0}
.empty-block{margin:var(--sp-2) 0 var(--sp-1);padding:var(--pan-x) var(--sp-4);border-top:1px solid var(--rule);border-bottom:1px solid var(--rule);color:var(--soft);font:400 16px/1.2 Anton,Impact,sans-serif;text-transform:uppercase;letter-spacing:.04em}
.road{display:grid;grid-template-columns:repeat(5,minmax(max-content,1fr));gap:var(--sp-1);flex:1;min-height:92px}
.step{display:flex;flex-direction:column;justify-content:flex-end;gap:var(--sp-1);padding:var(--sp-3) var(--sp-4);background:var(--wash);border-bottom:3px solid var(--empty);font-size:13px;line-height:1.2;min-width:0}
.step b{font-size:22px;line-height:1}.step>span{font-size:12px;font-weight:500;white-space:nowrap}.step small{color:var(--soft);font-size:13px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.step.on{border-bottom-color:var(--sc)}
.pips{display:flex;gap:var(--sp-1)}.pips i{display:block;flex:1;max-width:9px;height:7px;background:var(--empty)}.pips i.on{background:var(--sc)}
.st-axis,.st{display:grid;grid-template-columns:22px minmax(128px,1fr) minmax(24px,72px) 68px;gap:0 8px;align-items:center}
.st-axis{position:sticky;top:0;z-index:1;padding:0 var(--row-x) var(--sp-2);background:var(--page);color:var(--soft);font-size:13px;line-height:1.2}
.st-axis span:last-child{text-align:right}
.st-list{overflow-y:auto;min-height:0;flex:1;max-height:420px}
.st-list,.feed-body{scrollbar-gutter:stable;padding-right:var(--sp-2)}
.st-list,.feed-body{scrollbar-width:thin;scrollbar-color:var(--rule) transparent}
.st-list::-webkit-scrollbar,.feed-body::-webkit-scrollbar{width:8px}
.st-list::-webkit-scrollbar-thumb,.feed-body::-webkit-scrollbar-thumb{background:var(--rule);border-radius:4px}
.st-list::-webkit-scrollbar-thumb:hover,.feed-body::-webkit-scrollbar-thumb:hover{background:var(--soft)}
.st{padding:var(--row-y) var(--row-x);border-top:1px solid var(--rule);font-size:13px;line-height:1.3;min-height:44px}
.st:hover{background:color-mix(in srgb,var(--ink) 6%,transparent)}
.rank{font-size:16px;font-weight:700;text-align:center;line-height:1}
.who{min-width:0}.who b{display:block;font-size:14px;font-weight:700;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.who small{display:block;color:var(--soft);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.ptype{display:inline-flex;align-items:center;gap:var(--pill-gap);color:var(--ink);font-weight:500;min-width:0}
.ptype{max-width:100%}.ptype .nm{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0}.ptype i{display:block;flex:none;width:8px;height:8px;background:var(--ac)}
.track{display:block;height:12px;background:var(--wash)}.fill{display:flex;height:100%}.fill i{display:block;min-width:2px}
.fill .draft{background:var(--soft)}.fill .wait{background:var(--partial)}.fill .rev{background:var(--accent)}.fill .done{background:var(--good)}.fill .merged{background:var(--merged)}
.where{display:grid;justify-items:end;gap:var(--sp-1);color:var(--soft)}.where b{font:400 16px/1 Anton,Impact,sans-serif;color:var(--ink)}
.spark{display:block;width:100%;height:20px}.spline{vector-effect:non-scaling-stroke;fill:none;stroke:var(--soft);stroke-width:1.5;stroke-linejoin:round}.nodata{font-size:12px;color:var(--soft);white-space:nowrap}
.tabs{display:none;gap:var(--sp-1);flex-wrap:wrap;min-width:0}.js .tabs{display:flex}
.tabs button{padding:var(--sp-2) var(--sp-3);border:0;border-bottom:2px solid transparent;background:none;color:var(--soft);font:400 16px/1.2 Anton,Impact,sans-serif;cursor:pointer;border-radius:0}
.tabs button:hover{color:var(--ink)}.tabs button[aria-pressed=true]{border-bottom-color:var(--partial);color:var(--ink)}
.feed-body{overflow:auto;min-height:0;flex:1;max-height:640px}
.panel+.panel{margin-top:var(--sp-5)}.js .panel+.panel{margin-top:0}
.ptitle{display:flex;justify-content:space-between;gap:var(--sp-2);align-items:baseline;padding-bottom:var(--sp-2)}.ptitle span{color:var(--soft);font-size:13px}.js .ptitle{display:none}
.panel[hidden]{display:none}
.row{display:grid;grid-template-columns:30px minmax(0,1fr);gap:var(--sp-2);padding:var(--row-y) var(--row-x);border-top:1px solid var(--rule);font-size:13px;line-height:1.35}
.row:hover{background:color-mix(in srgb,var(--ink) 6%,transparent)}
.row .lead{color:var(--soft)}.row .lead b{display:block;color:var(--ink);font:400 15px/1.2 Anton,Impact,sans-serif}
.row .ttl{overflow-wrap:anywhere;font-size:14px}.row .ttl a{font-weight:500}
.sys{display:flex;flex-wrap:wrap;align-items:center;gap:var(--sp-1) var(--pill-gap);color:var(--soft);font-size:13px;margin-top:var(--sp-1)}
.av{width:20px;height:20px;background:var(--wash);color:transparent;overflow:hidden;flex:none}
.pill{display:inline-flex;align-items:center;gap:var(--sp-1);padding:2px var(--sp-2);font-size:13px;line-height:1.3;white-space:nowrap;font-weight:500;color:var(--soft)}
.pill:before{content:"";width:7px;height:7px;background:currentColor;flex:none}
.pill.ok{color:var(--good)}.pill.bad{color:var(--bad)}.pill.warn{color:var(--partial-text)}.pill.accent{color:var(--accent)}.pill.merged{color:var(--merged)}
.add{color:var(--good)}.del{color:var(--bad)}
.dup{padding:0 0 var(--sp-3) var(--btn-x);margin-bottom:var(--sp-3);border-left:4px solid var(--ac);border-bottom:1px solid var(--rule)}
.dup .chead{display:flex;flex-wrap:wrap;justify-content:space-between;gap:var(--sp-1) var(--sp-3);align-items:baseline}
.dup .row:first-of-type{border-top:0}
.chips-f{display:flex;flex-wrap:wrap;gap:var(--pill-gap);margin:var(--sp-2) 0}
.fchip{font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;padding:3px var(--sp-2);background:var(--wash);overflow-wrap:anywhere}
.heat{display:block;height:6px;background:var(--wash);margin:var(--sp-1) 0}.heat i{display:block;height:100%;background:var(--partial)}
.mono{font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;overflow-wrap:anywhere}
details.area{border-bottom:1px solid var(--rule)}
details.area>summary{cursor:pointer;padding:var(--row-y) var(--row-x);list-style:none;display:flex;gap:var(--sp-2);align-items:center;font:400 15px/1.3 Anton,Impact,sans-serif}
details.area>summary::-webkit-details-marker{display:none}details.area .cnt{margin-left:auto;font:400 13px Roboto,sans-serif;color:var(--soft)}
details.area>summary:before{content:"+";width:12px;color:var(--soft);font-weight:700}details.area[open]>summary:before{content:"\\2212"}
.a-meta{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:var(--row-y) var(--sp-4);padding:var(--sp-4) var(--pan-x)}
.a-meta .stat{padding:var(--sp-3) var(--sp-4);border-top:2px solid var(--ink)}
.stat{min-width:0}.stat span{display:block;color:var(--soft);font-size:13px;line-height:1.25}
.stat b{display:block;font-size:20px;line-height:1.15;white-space:nowrap}.stat.key b{color:var(--accent)}.stat.warn b{color:var(--partial-text)}
.stat small{display:block;color:var(--soft);font-size:13px;line-height:1.25}
.foot{grid-area:foot;padding:var(--sp-3) var(--pan-x);color:var(--soft);font-size:13px;line-height:1.35}
.small{color:var(--soft);font-size:13px;line-height:1.35}
.sm:not([hidden])~.sm:not([hidden]):before{content:", "}
.stf{grid-column:2/-1;display:flex;gap:var(--pill-gap);min-width:0;overflow:hidden;margin-top:var(--sp-1)}
.stf .fchip{flex:0 1 auto;min-width:0;max-width:34%;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;overflow-wrap:normal}
.tip{position:fixed;top:0;left:0;z-index:30;max-width:260px;padding:var(--sp-2) var(--sp-3);background:var(--tip);color:var(--tip-text);font-size:13px;line-height:1.3;pointer-events:none}
.tip[hidden]{display:none}
.vh{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
[hidden]{display:none!important}
@media (min-width:640px){.a-meta{grid-template-columns:repeat(4,minmax(0,1fr))}}
@media (max-width:760px){.road{grid-template-columns:1fr}.step{display:grid;grid-template-columns:44px minmax(0,1fr) auto;align-items:center;gap:var(--sp-2);padding:var(--sp-3) var(--sp-4);border-bottom-width:2px}.step .pips{display:none}}
@media (max-width:560px){.st{grid-template-columns:22px minmax(0,1fr) 68px}.st .track{grid-column:2/4;grid-row:2}.st-axis{display:none}.score{font-size:56px}}
@media (max-width:620px){.top{padding:var(--sp-3) var(--sp-4)}.top-right{width:100%;justify-content:space-between;margin-left:0}}
@media (min-width:900px){
:root{--fs:clamp(11px,calc(.42vw + .62vh + 4.2px),14px);--fs-s:max(10.5px,calc(var(--fs) - 1px));--gap:clamp(8px,1.4vw,22px);--pad:clamp(var(--sp-4),1.8vh,var(--sp-5))}
.js,.js body{height:100%;overflow:hidden}
body{font-size:var(--fs)}
.app{padding:0 var(--gap);column-gap:var(--gap);row-gap:clamp(var(--sp-1),.7vh,var(--sp-2))}
.js .app{height:100vh;height:100dvh;overflow:hidden;grid-template-rows:auto minmax(0,1.5fr) minmax(0,1fr) auto}
.top{gap:clamp(var(--sp-2),1.1vh,var(--sp-3));padding:clamp(var(--sp-3),1.4vh,var(--sp-4)) var(--sp-4)}
h1{font-size:clamp(20px,2vw,28px)}h2{font-size:clamp(13px,calc(var(--fs) + 1px),16px)}h3{font-size:var(--fs)}
.toolbar{justify-content:flex-start}
.toolbar input,.toolbar select,.seg button,.theme{font-size:var(--fs-s);padding:clamp(var(--sp-2),1vh,var(--sp-3)) clamp(var(--btn-x),.8vw,var(--sp-4))}
.ptabs button{font-size:clamp(15px,calc(var(--fs) + 3px),19px);padding:var(--sp-2) var(--sp-3)}
.toolbar input{flex:1 1 240px;min-width:200px;max-width:340px}.toolbar select{flex:0 1 clamp(130px,12vw,180px)}
.live,.result,.sec-note,.score-note,.legend,.step,.step small,.st-axis,.st,.row,.sys,.pill,.stat span,.stat small,.foot,.small,.head>span,.scene-cap,.tabs button{font-size:var(--fs-s)}
.a-city{padding:var(--sp-2) var(--sp-3)}
.a-trends,.a-road,.a-standings,.a-feed{padding:var(--pad) var(--pan-x)}
.js .a-city,.js .a-trends,.js .a-road,.js .a-standings,.js .a-feed{overflow:hidden}
.a-city{display:flex;flex-direction:column}.a-city>.scene{flex:1;min-height:0}
.js .pages{min-height:0}.js .page{flex:1;min-height:0}
.js .a-city,.js .a-trends,.js .a-standings,.js .a-meta{flex:1;min-height:0}
.head{margin-bottom:clamp(var(--sp-1),.6vh,var(--sp-2))}
.score{font-size:clamp(34px,7.4vh,72px)}.score small{font-size:.39em}
.score-note,.score-note b{font-size:var(--fs-s)}.score-note b{font-size:var(--fs)}
.charts{grid-template-columns:repeat(2,minmax(0,1fr))}
.js .charts{grid-template-rows:minmax(0,1.1fr) minmax(0,1fr)}
.js .chart-box .chart{position:absolute;left:0;top:0;width:100%;height:100%;margin:0}
.road{flex:none;min-height:0;grid-template-columns:repeat(5,minmax(0,1fr))}
.step{flex-direction:row;flex-wrap:wrap;align-items:baseline;justify-content:flex-start;gap:0 var(--sp-2);padding:var(--sp-3) var(--sp-4);overflow:hidden}.step .pips{display:none}.step b{font-size:clamp(15px,2.7vh,22px)}
.step>span{font-size:var(--fs-s);white-space:normal}
.st-axis,.st{grid-template-columns:clamp(14px,1.6vw,22px) minmax(118px,1.6fr) minmax(24px,.4fr) clamp(46px,5vw,68px)}
.st{min-height:clamp(34px,5vh,44px)}
.rank{font-size:clamp(13px,var(--fs),16px)}.who b{font-size:var(--fs)}
.st-list,.feed-body{max-height:none;overflow-y:auto}
.a-feed,.a-standings{display:flex;flex-direction:column}
.row .ttl{font-size:var(--fs)}
.a-meta{grid-template-columns:repeat(auto-fill,minmax(clamp(130px,15vw,230px),1fr));align-content:start;gap:clamp(var(--sp-2),1.8vh,var(--sp-5)) var(--gap);padding:var(--pad) var(--pan-x)}
.stat b{font-size:clamp(20px,4.6vh,40px)}.a-meta .stat{padding:var(--sp-3) var(--sp-4)}
.foot{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;padding:var(--sp-3) var(--pan-x)}
}
@media (min-width:1100px){
.app{grid-template-columns:minmax(0,1fr) 32%;grid-template-areas:"top top" "pages feed" "foot foot"}
.js .app{grid-template-rows:auto minmax(0,1fr) auto}
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
var tip=d.getElementById('tip');
function untitle(el){var t=el.querySelector('title');if(t)el.removeChild(t);el.removeAttribute('title');}
[].slice.call(d.querySelectorAll('[data-tip]')).forEach(untitle);
function place(e){var x=e.clientX+12,y=e.clientY+14,w=tip.offsetWidth,h=tip.offsetHeight;
if(x+w>innerWidth-8)x=Math.max(8,e.clientX-w-12);if(y+h>innerHeight-8)y=Math.max(8,e.clientY-h-12);
tip.style.left=x+'px';tip.style.top=y+'px';}
d.addEventListener('mouseover',function(e){var el=e.target.closest&&e.target.closest('[data-tip]');
if(!el){tip.hidden=true;return;}tip.textContent=el.getAttribute('data-tip');tip.hidden=false;place(e);});
d.addEventListener('mousemove',function(e){if(!tip.hidden)place(e);});
d.addEventListener('mouseleave',function(){tip.hidden=true;});
function qa(s,r){return [].slice.call((r||d).querySelectorAll(s));}
var tabs=qa('.tabs button'),panels=qa('.panel'),tmeta=d.getElementById('tabmeta'),tab='dup';
function showTab(n){tab=n;tabs.forEach(function(b){b.setAttribute('aria-pressed',String(b.getAttribute('data-tab')===n));});
panels.forEach(function(p){var on=p.getAttribute('data-panel')===n;p.hidden=!on;if(on&&tmeta)tmeta.textContent=p.getAttribute('data-meta')||'';});}
tabs.forEach(function(b){b.addEventListener('click',function(){showTab(b.getAttribute('data-tab'));write();});});
// time range: Python computed one dataset per range (towers, Areas rows, counts, tab texts); list rows are filtered by their timestamps
var RD=null;try{RD=JSON.parse(d.getElementById('range-data').textContent);}catch(e){}
var DP='progress',RR=(RD&&RD.ranges)||{},DEF=(RD&&RD.default)||'7d',range=DEF,page=DP,PGS=['progress','effort','areas','trends','stats'],cs=0;
var rseg=qa('#range button'),pbtn=qa('#ptabs button'),pages=qa('.page');
function inR(e){var c=e.getAttribute('data-closed');return c!==null?+c>=cs:(+e.getAttribute('data-created')>=cs||+e.getAttribute('data-updated')>=cs);}
function hideAll(sel,r){var n=0;qa(sel,r).forEach(function(e){var k=inR(e);e.hidden=!k;if(k)n++;});return n;}
function note(key,items,vis){var p=d.querySelector('.panel[data-panel='+key+']'),m=p&&p.querySelector('.range-empty');
if(m){m.textContent='No PRs in the last '+RR[range].rl+'.';m.hidden=!(items>0&&vis===0);}}
function applyRange(){var R=RR[range];rseg.forEach(function(b){b.setAttribute('aria-pressed',String(b.getAttribute('data-range')===range));});
if(!R)return;cs=RD.now-R.s;
qa('.dup').forEach(function(c){var n=0,op=0;qa('.row.rg',c).forEach(function(r){var k=inR(r);r.hidden=!k;if(k){n++;if(r.getAttribute('data-closed')===null)op++;}});
var sv=hideAll('.sm',c),sup=c.querySelector('.sup');if(sup)sup.hidden=!sv;c.hidden=n<2||!op;});
note('dup',qa('.dup').length,qa('.dup:not([hidden])').length);
hideAll('.row.rd');note('ready',qa('.row.rd').length,qa('.row.rd:not([hidden])').length);
var hs=qa('.row.ht'),mx=0;hs.forEach(function(h){var n=hideAll('a.rg',h);h.setAttribute('data-n',n);h.hidden=!n;if(n>mx)mx=n;var b=h.querySelector('.lead b');if(b)b.textContent=n;});
hs.forEach(function(h){var i=h.querySelector('.heat i');if(i)i.style.width=(mx?Math.round(100*(+h.getAttribute('data-n'))/mx):0)+'%';});
note('hot',hs.length,qa('.row.ht:not([hidden])').length);
hideAll('.row.cl');note('closed',qa('.row.cl').length,qa('.row.cl:not([hidden])').length);
panels.forEach(function(p){var k=p.getAttribute('data-panel');if(k==='all'||!R.metas[k])return;p.setAttribute('data-meta',R.metas[k]);
var s=p.querySelector('.ptitle span');if(s)s.textContent=R.metas[k];});
if(sel)qa('option',sel).forEach(function(o){var n=R.sel&&R.sel[o.getAttribute('value')],nm=o.getAttribute('data-name');if(nm&&n!=null)o.textContent=nm+' ('+n+')';});
var sl=d.getElementById('standings');if(sl){sl.innerHTML=R.standings;qa('[data-tip]',sl).forEach(untitle);}
var rl=d.getElementById('range-line');if(rl)rl.textContent=R.line;
var wk=d.getElementById('pg-work'),wl=d.getElementById('pg-rl');if(wk&&R.work!=null)wk.innerHTML=R.work;if(wl)wl.textContent=R.rl;
qa('[data-rstat]').forEach(function(e){var v=R.counts[e.getAttribute('data-rstat')],sp=e.querySelector('span'),b=e.querySelector('b');
if(sp)sp.textContent=e.getAttribute('data-base')+' ('+R.rl+')';if(b)b.textContent=v==null?'?':v;});
qa('.legend .k-merged').forEach(function(e){var n=e.parentNode.lastChild;if(n)n.textContent='Merged in '+R.rl;});
root.setAttribute('data-range',range);
try{d.dispatchEvent(new CustomEvent('prscan-range',{detail:range}));}catch(e){}}
function showPage(n){page=n;pbtn.forEach(function(b){b.setAttribute('aria-pressed',String(b.getAttribute('data-page')===n));});
pages.forEach(function(p){p.hidden=p.getAttribute('data-page')!==n;});}
var F=d.getElementById('filters');
var rows=qa('.pr'),groups=qa('details.area'),
q=d.getElementById('q'),sel=d.getElementById('area'),res=d.getElementById('result'),segs=qa('#states button');
var st={q:'',area:'',state:''},STATES=['unrev','done','draft','closed'],fcs='';
function dec(s){try{return decodeURIComponent(s.replace(/\\+/g,' '))}catch(e){return ''}}
function opt(v){if(!sel||!v)return null;for(var i=0;i<sel.options.length;i++)if(sel.options[i].value===v)return sel.options[i];return null;}
function parse(){st={q:'',area:'',state:''};var tt='';range=DEF;page=DP;fcs='';
(location.hash||'').replace(/^#/,'').split('&').forEach(function(kv){var i=kv.indexOf('=');if(i<1)return;
var k=kv.slice(0,i),v=dec(kv.slice(i+1));if(k==='q'||k==='area')st[k]=v;else if(k==='tab')tt=v;
else if(k==='range'){if(RR[v])range=v;}else if(k==='focus'){fcs=v;}else if(k==='page'){if(PGS.indexOf(v)>=0)page=v;}
else if(k==='state'){if(STATES.indexOf(v)>=0)st.state=v;}else if(STATES.indexOf(k)>=0&&v==='1'&&!st.state)st.state=k;});
if(sel&&st.area&&!opt(st.area))st.area='';return tt;}
function on(){return !!(st.q.trim()||st.area||st.state);}
function write(){var p=[];if(st.q)p.push('q='+encodeURIComponent(st.q));if(st.area)p.push('area='+encodeURIComponent(st.area));
if(st.state)p.push('state='+st.state);if(tab!=='dup')p.push('tab='+tab);if(range!==DEF)p.push('range='+range);if(page!==DP)p.push('page='+page);if(fcs)p.push('focus='+encodeURIComponent(fcs));
history.replaceState(null,'',p.length?'#'+p.join('&'):location.pathname+location.search);}
function areaOk(r){if(!st.area)return true;var a=r.getAttribute('data-area'),o=opt(st.area),m=o&&o.getAttribute('data-members');
return m?(','+m+',').indexOf(','+a+',')>=0:a===st.area;}
function isCl(r){return r.getAttribute('data-cl')==='1';}
function apply(){var t=st.q.toLowerCase().trim(),f=on(),shown=0,wc=st.state==='closed',pool=0;
rows.forEach(function(r){var k=isCl(r)===wc&&(!RD||inR(r));if(k)pool++;
var ok=k&&(!t||r.getAttribute('data-q').indexOf(t)>=0)&&areaOk(r)&&(!st.state||wc||r.getAttribute('data-'+st.state)==='1');
r.hidden=!ok;if(ok)shown++;});
groups.forEach(function(g){var n=g.querySelectorAll('.pr:not([hidden])').length,c=g.querySelector('.cnt'),tot=0;
[].forEach.call(g.querySelectorAll('.pr'),function(r){if(isCl(r)===wc&&(!RD||inR(r)))tot++;});
g.hidden=n===0;if(f)g.open=n>0;if(c)c.textContent=f?n+' of '+tot:String(tot);});
segs.forEach(function(b){b.setAttribute('aria-pressed',String(b.getAttribute('data-state')===st.state));});
if(res)res.textContent=f?shown+' of '+pool:'';
var allp=d.querySelector('.panel[data-panel=all]');if(allp&&f)allp.setAttribute('data-meta','Showing '+shown+' of '+pool);
else if(allp)allp.setAttribute('data-meta',pool+' pull requests');
if(RD&&RR[range])note('all',rows.length,pool);
showTab(tab);}
function sendFocus(){root.setAttribute('data-focus',fcs);try{d.dispatchEvent(new CustomEvent('prscan-focus',{detail:fcs}));}catch(e){}}
function sync(){if(q)q.value=st.q;if(sel)sel.value=st.area;applyRange();showPage(page);apply();sendFocus();}
function changed(){if(on())tab='all';write();apply();}
if(F){q.addEventListener('input',function(){st.q=q.value;changed();});
if(sel)sel.addEventListener('change',function(){st.area=sel.value;changed();});
segs.forEach(function(b){b.addEventListener('click',function(){st.state=b.getAttribute('data-state')||'';changed();});});}
rseg.forEach(function(b){b.addEventListener('click',function(){range=b.getAttribute('data-range');applyRange();apply();write();});});
pbtn.forEach(function(b){b.addEventListener('click',function(){showPage(b.getAttribute('data-page'));write();});});
// the map reports a focus change (card, tower, Esc, Back): the hash follows; a bar on the Areas page switches to Effort and focuses that tower
d.addEventListener('prscan-focus-set',function(e){fcs=String(e.detail||'');root.setAttribute('data-focus',fcs);write();});
function barFocus(e){var tr=e.target.closest&&e.target.closest('.st .track'),li=tr&&tr.closest('.st'),sg=li&&li.getAttribute('data-slug');if(!sg)return false;
e.preventDefault();fcs=sg;showPage('effort');write();sendFocus();return true;}
d.addEventListener('click',barFocus);d.addEventListener('keydown',function(e){if(e.key==='Enter'||e.key===' ')barFocus(e);});
// an area link keeps the time range and the page: it sets the area filter instead of replacing the hash
d.addEventListener('click',function(e){var a=e.target.closest&&e.target.closest('a.ptype'),h=a&&a.getAttribute('href');
if(!h||h.indexOf('#area=')!==0||!sel)return;e.preventDefault();st.area=dec(h.slice(6));if(!opt(st.area))st.area='';sel.value=st.area;changed();});
window.addEventListener('hashchange',function(){var tt=parse();tab=tt||(on()?'all':tab);sync();var el=d.getElementById('feed');if(el&&on())el.scrollIntoView();});
var t0=parse();tab=t0||(on()?'all':'dup');sync();
})();
"""


def stat(label, value, delta=None, show_delta=True, cls="", rstat=None):
    """rstat: (key, base label) of a stat that follows the time range; the JS rewrites its label and value."""
    return '<div class="stat%s"%s><span>%s</span><b>%s</b>%s</div>' % (
        " " + cls if cls else "", ' data-rstat="%s" data-base="%s"' % (esc(rstat[0]), esc(rstat[1])) if rstat else "",
        esc(label), esc(value), delta_text(delta) if show_delta else "")


def ptype(area, link=False):
    inner = '<i></i><span class="nm">%s</span>' % esc(area_label(area))
    if link:
        inner = '<a class="ptype" title="%s" href="#area=%s">%s</a>' % (esc(area_label(area)), esc(urllib.parse.quote(slug(area), safe="-")), inner)
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

TOP_AREAS = 7
ELSE_NAME = "Other"     # the rolled-up tower and row; the unmapped-path area is labelled the same
ROLLUP_SLUG = "group-other"   # select value of the rolled-up entry: "area-other" is the unmapped-path area
MERGED_LABEL = "Merged in " + DEFAULT_RL     # the SVG fallback and the default range; the canvas relabels per range
SCENE_CAP = 40          # tallest tower in layers; above it the layers are scaled
SCENE_W, SCENE_H = 720, 520   # viewBox of the static SVG
SCENE_YAW = -22.0       # degrees, fixed angle of the SVG and centre of the canvas pan
SCENE_ARC = 90.0        # the canvas pans inside SCENE_YAW +/- SCENE_ARC degrees (a 180 degree arc), never spins
SCENE_ZOOM = 0.85       # the scene is drawn at this fraction of the size that fits, centred
SCENE_SX, SCENE_PY = 0.87, 0.36   # screen x per cell, screen y per cell on the ground (fixed pitch)
TOWER_CELLS, TOWER_PITCH = 2, 3   # footprint of a tower and distance between towers, in cells
SCENE_MX, SCENE_MY = 2.2, 1.8     # ground plate margin around the tower row, in cells
SCENE_GHOST = ("wait", "draft")   # layers not reviewed yet are drawn as ghost voxels
SCENE_PAD_X, SCENE_PAD_Y = 10, 8    # inner padding between a callout's text and its box edge (SVG and canvas)
SCENE_BOX_H = 61        # callout box height: cap height above the first baseline (11) + 3 lines + descender (3) + 2 * SCENE_PAD_Y
SCENE_BAND_TOP = 4      # top of the callout row
SCENE_EDGE = 8          # margin of the callout row at both panel edges
SCENE_GAP = 8           # space between neighbouring callout cards
SCENE_LEAD = 10         # space between the bottom of the cards and the first scene pixel
SCENE_MIN_SLOT = 110    # a callout slot narrower than this switches the cards to compact mode (name and open count)
SCENE_COMPACT_H = 46    # compact card height: cap height (11) + 2 lines + descender (3) + 2 * SCENE_PAD_Y
SCENE_FOCUS_MS = 600    # duration of the focus transition (ease-out)
SCENE_FOCUS_FILL = 0.7  # the focused tower fills this fraction of the stage height
SCENE_FOCUS_GAP = 0.8   # vertical gap between the state layers of the focused tower, in layers
SCENE_LINE = 15         # extra height of the callouts for the "merged / closed" line (when closed PRs are known)
CHART_W, CHART_H = 280, 150   # viewBox of the trend charts: close to their usual box, so the text keeps its size
SCENE_STATES = [("merged", MERGED_LABEL, "--merged"), ("done", "Looks done", "--good"), ("rev", "Reviewed", "--accent"),
                ("wait", "Waiting for review", "--partial"), ("draft", "Draft", "--soft")]


def entry_slug(e):
    return ROLLUP_SLUG if e.get("rollup") else slug(e["name"])


def top_areas(an, n=TOP_AREAS):
    """The n groups or ungrouped areas with the most open PRs (ties by merged, closed, then name) as
    [{"name", "prs", "merged", "closed", "members"}], plus one rolled-up "Other" entry (rollup True, members = the areas
    in it, sorted like the area ranking) when there are more than n. prs = open PRs; merged and closed = PRs merged and
    closed without a merge in the last CLOSED_DAYS days (an["closed"], empty when unknown). members is None for an
    ungrouped area and the member areas that have open or closed PRs for a group."""
    closed = an.get("closed") or []
    by_key, m_of, c_of = {}, {}, {}
    seen = set(an["areas"]) | {p["primary"] for p in closed}
    for a in seen:
        by_key.setdefault(group_of(a), []).append(a)
    for p in closed:
        (m_of if p.get("merged") else c_of).setdefault(p["primary"], []).append(p)
    grouped = {name for name, _ in AREA_GROUPS}
    rank = lambda a: (-len(an["areas"].get(a, [])), -len(m_of.get(a, [])), -len(c_of.get(a, [])), a)
    entries = []
    for key, areas in by_key.items():
        areas = sorted(areas, key=rank)
        entries.append({"name": key, "prs": [p for a in areas for p in an["areas"].get(a, [])],
                        "merged": [p for a in areas for p in m_of.get(a, [])], "closed": [p for a in areas for p in c_of.get(a, [])],
                        "members": areas if key in grouped else None})
    entries.sort(key=lambda e: (-len(e["prs"]), -len(e["merged"]), -len(e["closed"]), e["name"]))
    # the unmapped-path area is never a tower of its own: it always rolls into "Other"
    unmapped = [e for e in entries if e["name"] == OTHER]
    entries = [e for e in entries if e["name"] != OTHER]
    out, rest = entries[:n], entries[n:] + unmapped
    if rest:
        members = sorted((a for e in rest for a in (e["members"] or [e["name"]])), key=rank)
        out.append({"name": ELSE_NAME, "rollup": True, "prs": [p for e in rest for p in e["prs"]],
                    "merged": [p for e in rest for p in e["merged"]], "closed": [p for e in rest for p in e["closed"]],
                    "members": members})
    return out


def effort_scene(an, n=TOP_AREAS, cap=SCENE_CAP, rl=DEFAULT_RL):
    """Tower data for the effort map: one tower per top_areas entry, layers bottom to top (merged in the range rl, looks done, reviewed, waiting, draft), h = voxel layers of a run. Above cap layers for the tallest
    tower, h is scaled by cap/tallest (every non-empty run keeps at least one layer) and per = PRs per layer is stated in
    the caption. Closed PRs that were not merged are counted in the callout (closed) but not stacked. hasClosed is False
    when the closed PRs could not be fetched."""
    entries = top_areas(an, n)
    has_closed = an.get("closed") is not None
    tallest = max([len(e["prs"]) + len(e["merged"]) for e in entries] or [0])
    factor = cap / tallest if tallest > cap else 1.0
    towers = []
    for e in entries:
        ps = e["prs"]
        layers = []
        for k, label, _ in SCENE_STATES:
            c = len(e["merged"]) if k == "merged" else sum(bucket(p) == k for p in ps)
            if c:
                layers.append({"k": k, "label": "Merged in " + rl if k == "merged" else label, "n": c, "ghost": k in SCENE_GHOST,
                               "h": c if factor == 1.0 else max(1, round(c * factor))})
        by_state = {k: [p["number"] for p in (e["merged"] if k == "merged" else ps) if k == "merged" or bucket(p) == k]
                    for k, _, _ in SCENE_STATES}
        towers.append({"name": area_label(e["name"]), "slug": entry_slug(e), "members": e["members"] or [], "open": len(ps),
                       "prs": {k: sorted(v, reverse=True) for k, v in by_state.items() if v},
                       "files": [[f, n] for f, n, _ in top_files(ps + e["merged"])],
                       "merged": len(e["merged"]), "closed": len(e["closed"]),
                       "unreviewed": sum(bool(p["unreviewed"]) for p in ps),
                       "add": sum(p.get("additions", 0) for p in ps), "del": sum(p.get("deletions", 0) for p in ps),
                       "layers": layers, "height": sum(ly["h"] for ly in layers)})
    per = tallest / cap if factor != 1.0 else 1.0
    noun = "PR" if any(t["merged"] for t in towers) else "open PR"
    if not towers:
        caption = ""
    elif factor == 1.0:
        caption = "One layer is one %s." % noun
    else:
        caption = "One layer is about %s %ss: the layers are scaled so the tallest tower fits in %d." % (
            ("%.1f" % per).rstrip("0").rstrip("."), noun, cap)
    if caption:
        caption += " Solid layers have moved forward (%slooks done, reviewed); outlined ghost layers still wait for review or are drafts." % (
            "merged, " if has_closed else "")
        if has_closed:
            caption += " The violet base is the PRs merged in the last %s; closed PRs that were not merged are not stacked." % rl
    return {"towers": towers, "scale": factor, "per": per, "cap": cap, "caption": caption, "hasClosed": has_closed, "rl": rl}


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


def _crosses(a, b, c, d):
    """True when the segments a-b and c-d properly cross (touching ends and collinear overlaps do not count)."""
    def o(p, q, r):
        return (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    return o(a, b, c) * o(a, b, d) < 0 and o(c, d, a) * o(c, d, b) < 0


def callout_layout(tower_xs, panel_w, tower_ys=None, card_y=0.0, pad=SCENE_EDGE, min_slot=SCENE_MIN_SLOT):
    """Callout cards in one row, equal slots across the panel width. Returns the slots left to right as
    [{"i": tower index, "x0", "cx", "w": slot geometry, "compact": slot narrower than min_slot}]. The cards are ordered by the
    towers' projected top-centre x (ties by index), so the leader lines (card bottom centre (cx, card_y) to the tower top
    (x, y)) rarely cross. Order by x alone does not guarantee it when the tops differ in height (towers nearly in line
    with the camera), so with tower_ys given, crossing pairs are swapped until none is left: each swap shortens the total
    length, so it ends. The JS callout layout (calloutLayout) does the same."""
    n = len(tower_xs)
    if not n:
        return []
    sw = (panel_w - 2 * pad) / float(n)
    order = sorted(range(n), key=lambda i: (tower_xs[i], i))
    cx = [pad + (k + 0.5) * sw for k in range(n)]
    if tower_ys is not None:
        def seg(k):
            i = order[k]
            return (cx[k], card_y), (tower_xs[i], tower_ys[i])
        changed = True
        while changed:
            changed = False
            for k in range(n):
                for m in range(k + 1, n):
                    if _crosses(*seg(k), *seg(m)):
                        order[k], order[m] = order[m], order[k]
                        changed = True
    return [{"i": i, "x0": pad + k * sw, "cx": cx[k], "w": sw, "compact": sw < min_slot} for k, i in enumerate(order)]


def scene_summary(scene):
    ts = scene["towers"]
    hc = scene.get("hasClosed")
    return "Effort map, open pull requests per area: " + "; ".join(
        "%s %d open, %d unreviewed%s" % (t["name"], t["open"], t["unreviewed"],
                                          ", " + merged_line(t, rl=scene.get("rl", DEFAULT_RL)) if hc else "") for t in ts) + "."


def merged_line(t, short=False, rl=DEFAULT_RL):
    """The callout line for the closed PRs of a tower, e.g. "3 merged \u00b7 2 closed (7 d)"."""
    return "%d merged \u00b7 %d closed%s" % (t["merged"], t["closed"], "" if short else " (%s)" % rl)


def scene_key_html(scene):
    """The list under the map for narrow boxes: one item per tower."""
    hc = scene.get("hasClosed")
    return "".join('<li>%s<small>%d open%s</small></li>' % (
        ptype(t["name"]) if not t["members"] else '<span class="ptype" style="--ac:var(--soft)"><i></i><span class="nm">%s</span></span>' % esc(t["name"]),
        t["open"], " &middot; %d merged" % t["merged"] if hc else "") for t in scene["towers"])


def scene_range(scene):
    """What the canvas swaps when the range changes: towers, caption, plate size, label and the narrow-box key."""
    gx, gy = scene_plate(len(scene["towers"]))
    return {"towers": scene["towers"], "per": scene["per"], "rl": scene.get("rl", DEFAULT_RL),
            "hasClosed": bool(scene.get("hasClosed")), "gx": gx, "gy": gy, "key": scene_key_html(scene)}


def scene_titles(by_num, ranges):
    """The shared {number: [title, url, author]} map of every PR the towers list, for all ranges. Values are HTML-escaped
    (the focus panel inserts them as markup) and the link is safe_url'd."""
    nums = sorted({n for r in ranges.values() for t in r["towers"] for v in t["prs"].values() for n in v})
    return {str(n): [esc(by_num[n]["title"]), esc(safe_url(by_num[n]["url"])), esc(by_num[n].get("author", ""))] for n in nums if n in by_num}


def scene_json(scene, ranges=None, titles=None):
    """Inline JSON for the canvas renderer; every '<' is escaped so nothing can close the script block. ranges: {range key:
    scene_range(...)}, the datasets the canvas swaps between (the top-level towers are the default range's). titles:
    scene_titles(...), shared by all ranges."""
    data = {"towers": scene["towers"], "per": scene["per"], "caption": scene["caption"], "hasClosed": bool(scene.get("hasClosed")),
            "days": scene.get("rl", DEFAULT_RL),
            "states": [{"k": k, "label": lb, "v": v, "ghost": k in SCENE_GHOST} for k, lb, v in SCENE_STATES],
            "geom": {"w": TOWER_CELLS, "pitch": TOWER_PITCH, "sx": SCENE_SX, "py": SCENE_PY, "yaw": SCENE_YAW,
                     "arc": SCENE_ARC, "zoom": SCENE_ZOOM, "gx": scene_plate(len(scene["towers"]))[0],
                     "gy": scene_plate(len(scene["towers"]))[1], "minSlot": SCENE_MIN_SLOT, "edge": SCENE_EDGE, "gap": SCENE_GAP,
                     "ms": SCENE_FOCUS_MS, "fill": SCENE_FOCUS_FILL, "layerGap": SCENE_FOCUS_GAP}}
    if titles is not None:
        data["titles"] = titles
    if ranges is not None:
        data["ranges"] = ranges
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
    pad, px = 12, SCENE_PAD_X
    compact = (SCENE_W - 2 * SCENE_EDGE) / float(n) < SCENE_MIN_SLOT
    extra = SCENE_LINE if scene.get("hasClosed") and not compact else 0
    card_h = SCENE_COMPACT_H if compact else SCENE_BOX_H + extra
    band_bottom = SCENE_BAND_TOP + card_h + SCENE_LEAD
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
    # callouts in one row, ordered by the towers' x on screen: the leader lines do not cross
    slots = callout_layout([tops[i][0] for i in range(n)], SCENE_W, [tops[i][1] - 3 for i in range(n)], SCENE_BAND_TOP + card_h)
    o.append('<g class="callouts">')
    for sl in slots:
        i, t = sl["i"], ts[sl["i"]]
        bx, bw = sl["x0"] + SCENE_GAP / 2.0, sl["w"] - SCENE_GAP
        x, room = bx + px, bw - 2 * px
        name, shown = t["name"], t["name"]
        while len(shown) > 1 and len(shown) * 7.8 > room:
            shown = shown[:-1]
        if shown != name:
            shown = shown[:-1] + "\u2026"
        y0 = SCENE_BAND_TOP + 19
        tx, ty = tops[i]
        pts = "%.1f,%.1f %.1f,%.1f" % (sl["cx"], SCENE_BAND_TOP + card_h, tx, ty - 3)
        title = esc("%s: %d open, %d unreviewed%s" % (name, t["open"], t["unreviewed"], ", " + merged_line(t, rl=scene.get("rl", DEFAULT_RL)) if scene.get("hasClosed") else ""))
        o.append('<g class="callout"><title>%s</title><polyline points="%s"/><rect class="co-bg" x="%.1f" y="%d" width="%.1f" height="%d"/>' % (
            title, pts, bx, SCENE_BAND_TOP, bw, card_h))
        if compact:
            o.append('<text x="%.1f" y="%d"><tspan class="co-nm" x="%.1f">%s</tspan>'
                     '<tspan class="co-n" x="%.1f" dy="16"><tspan class="co-num">%d</tspan><tspan class="co-w"> open</tspan></tspan></text></g>' % (
                         x, y0, x, esc(shown), x, t["open"]))
            continue
        l2 = "%d open \u00b7 %d unreviewed" % (t["open"], t["unreviewed"])
        if len(l2) * 6.4 > room:
            l2 = "%d open \u00b7 %d unrev." % (t["open"], t["unreviewed"])
        l4 = ""
        if extra:
            l4 = merged_line(t, rl=scene.get("rl", DEFAULT_RL))
            if len(l4) * 6.4 > room:
                l4 = merged_line(t, True)
        o.append('<text x="%.1f" y="%d"><tspan class="co-nm" x="%.1f">%s</tspan>'
                 '<tspan class="co-n" x="%.1f" dy="16"><tspan class="co-num">%d</tspan><tspan class="co-w"> open</tspan>'
                 '<tspan class="co-sub">%s</tspan></tspan>'
                 '<tspan class="co-sub" x="%.1f" dy="15"><tspan class="add">+%s</tspan> <tspan class="del">\u2212%s</tspan></tspan>%s</text></g>' % (
                     x, y0, x, esc(shown), x, t["open"], esc(l2[len("%d open" % t["open"]):]), x, _short(t["add"]), _short(t["del"]),
                     '<tspan class="co-sub" x="%.1f" dy="15">%s</tspan>' % (x, esc(l4)) if l4 else ""))
    o.append("</g></svg>")
    return "".join(o)


SCENE_CSS = """
.scene{position:relative;display:flex;flex-direction:column;min-width:0;min-height:0;container-type:inline-size}
.stage{position:relative;flex:none;width:100%;height:min(60vh,420px)}
.scene-svg,.scene-cv{display:block;position:absolute;left:0;top:0;width:100%;height:100%}
.scene-cv{touch-action:pan-y;cursor:grab;outline-offset:-2px}.scene-cv:active{cursor:grabbing}
.scene .score-box{margin-bottom:var(--pill-gap)}
.scene-foot{display:flex;flex-wrap:wrap;align-items:center;gap:var(--sp-1) var(--sp-4);margin-top:var(--sp-1)}
.plate{fill:var(--ink);fill-opacity:.07;stroke:var(--ink);stroke-opacity:.2;stroke-width:1;stroke-linejoin:round}
.co-bg{fill:var(--surface)}
.vx{--c:var(--ink)}.v-done{--c:var(--good)}.v-rev{--c:var(--accent)}.v-wait{--c:var(--partial)}.v-draft{--c:var(--soft)}.v-merged{--c:var(--merged)}
.vx{fill:var(--c);stroke:none}
.f-t{fill:color-mix(in srgb,var(--c) 68%,#fff)}.f-l{fill:var(--c)}.f-r{fill:color-mix(in srgb,var(--c) 66%,#000)}
.vx.ghost{fill:var(--c);fill-opacity:.16;stroke:var(--c);stroke-width:1;stroke-linejoin:round}
.seam{fill:none;stroke:var(--page);stroke-width:.7;opacity:.85}.gseam{fill:none;stroke:var(--c);stroke-width:.7;opacity:.5}
.callout polyline{fill:none;stroke:var(--ink);stroke-width:1}
.scene-svg text{font:400 12px Roboto,Arial,sans-serif;fill:var(--ink);paint-order:stroke;stroke:var(--page);stroke-width:3px;stroke-linejoin:round}
.scene-svg .co-nm{font:400 14px Anton,Impact,sans-serif;fill:var(--ink)}.scene-svg .co-n{font-weight:500;fill:var(--ink)}
.scene-svg .add{fill:var(--good)}.scene-svg .del{fill:var(--bad)}
.cos{position:absolute;left:0;top:0;right:0;z-index:1;pointer-events:none}
.cd{position:absolute;left:0;top:0;box-sizing:border-box;margin:0;padding:var(--sp-2) 10px;overflow:hidden;pointer-events:auto;text-align:left;cursor:pointer;
background:var(--surface);color:var(--ink);border:0;border-radius:0;font:500 12px/15px Roboto,Arial,sans-serif;transition:opacity .25s}
.cd:hover,.cd:focus-visible{outline:2px solid var(--accent);outline-offset:-2px}
.cd>span{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.cd-nm{font:400 14px/15px Anton,Impact,sans-serif}.cd-n b{font-weight:500}
.cd .add,.cd .del{font-style:normal}.cd .add{color:var(--good)}.cd .del{color:var(--bad)}
.cos.c .cd-s,.cos.c .cd-d,.cos.c .cd-m{display:none}
.cos.t .cd-nm,.cos.t .cd-w,.cos.t .cd-s,.cos.t .cd-d,.cos.t .cd-m{display:none}.cos.t .cd-n b{font:400 22px/20px Anton,Impact,sans-serif}
.foc .cd{opacity:0;visibility:hidden;pointer-events:none;transition:opacity .25s,visibility 0s .25s}
.fpanel{position:absolute;z-index:2;top:0;right:0;bottom:0;width:min(380px,42%);box-sizing:border-box;display:flex;flex-direction:column;gap:var(--sp-3);padding:var(--sp-4);
background:var(--surface);border-left:1px solid var(--edge);transform:translateX(100%);opacity:0;transition:transform .6s cubic-bezier(.2,.7,.2,1),opacity .6s ease-out}
.fpanel[hidden]{display:none}.fpanel.on{transform:none;opacity:1}
.fp-head{display:flex;align-items:center;gap:var(--sp-3)}.fp-head h3{margin:0;min-width:0;font:400 22px/1.15 Anton,Impact,sans-serif;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.fp-back{flex:none;font:500 13px/1 Roboto,Arial,sans-serif;color:var(--ink);background:transparent;border:1px solid var(--rule);padding:var(--sp-2) var(--sp-3);cursor:pointer}
.fp-back:hover,.fp-back:focus-visible{border-color:var(--accent);outline:none}
.fp-body{display:flex;flex-direction:column;gap:var(--sp-3);min-height:0;flex:1;font-size:13px;line-height:1.35}
.fp-body h4{margin:0 0 var(--sp-1);font:500 12px/1.3 Roboto,Arial,sans-serif;text-transform:uppercase;letter-spacing:.04em;color:var(--soft)}
.fp-body p,.fp-body ul,.fp-body ol{margin:0;padding:0;list-style:none}.fp-mem{color:var(--soft)}
.fp-counts li{display:flex;align-items:center;gap:var(--sp-2);padding:2px 0}.fp-counts li span{flex:1;min-width:0}.fp-counts b{font-weight:500}
.fp-sw{flex:none;width:10px;height:10px;background:var(--c,var(--rule))}
.fp-files li{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--soft)}
.fp-prs{flex:1;min-height:0;overflow:auto;overscroll-behavior:contain}
.fp-prs ol{margin-bottom:var(--sp-3)}.fp-prs li{padding:2px 0}.fp-prs a{color:var(--accent);font-weight:500}
.st .track{cursor:pointer}.st .track:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
@container (max-width:560px){.fpanel{top:auto;left:0;width:auto;height:58%;border-left:0;border-top:1px solid var(--edge);transform:translateY(100%)}}
@media (prefers-reduced-motion:reduce){.fpanel,.cd{transition:none}}
.scene-cap{color:var(--soft);font-size:13px;line-height:1.35;flex:1 1 240px;min-width:0}
.scene .legend{margin:0}
.scene .legend .sw.k-wait{background:color-mix(in srgb,var(--partial) 18%,transparent);box-shadow:inset 0 0 0 1px var(--partial)}
.scene .legend .sw.k-draft{background:color-mix(in srgb,var(--soft) 18%,transparent);box-shadow:inset 0 0 0 1px var(--soft)}
.scene-key{display:none;margin:var(--pill-gap) 0 0}.scene-key li{display:flex;justify-content:space-between;gap:var(--sp-2);padding:var(--row-y) var(--row-x);border-top:1px solid var(--rule);font-size:13px}
.scene-key small{color:var(--soft)}
@media (min-width:900px){.stage{flex:1;height:auto;min-height:0}.scene-cap{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
@container (max-width:360px){
.scene-svg .co-nm,.scene-svg .co-sub,.scene-svg .co-w,.scene-svg .co-bg{display:none}.scene-svg .co-n{font:400 30px Anton,Impact,sans-serif}
.scene-key{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));column-gap:var(--sp-3)}}
@media (max-width:899px){@container (max-width:540px){
.scene-svg .co-nm,.scene-svg .co-sub,.scene-svg .co-w,.scene-svg .co-bg{display:none}.scene-svg .co-n{font:400 30px Anton,Impact,sans-serif}
.scene-key{display:block}}}
"""

SCENE_JS = r"""
(function(){
var d=document,stage=d.getElementById('stage'),dEl=d.getElementById('scene-data'),tip=d.getElementById('tip');
if(!stage||!dEl)return;
var svg=stage.querySelector('svg'),data;
try{data=JSON.parse(dEl.textContent);}catch(e){return;}
var T=data.towers||[],N=T.length,S=data.states||[],G=data.geom||{},HC=!!data.hasClosed,DAYS=data.days||'7 d',EX=0,RG=data.ranges||{},TL=data.titles||{};
var cv=d.createElement('canvas'),g=cv.getContext&&cv.getContext('2d');
if(!g)return;
cv.className='scene-cv';cv.tabIndex=0;cv.setAttribute('role','img');
cv.setAttribute('aria-label',((svg&&svg.getAttribute('aria-label'))||'Effort map')+' Drag or press Left and Right to pan. Click a tower or a card to focus it.');
if(svg)stage.replaceChild(cv,svg);else stage.appendChild(cv);
// Layers: layer (ground plate, towers, leader lines; rebuilt when the yaw, the size or the focus changes), cv (transparent composite with the hover highlight).
// The page gradient shows through both canvases: there is no backdrop to paint. The callout cards are buttons over the canvas.
function mk(){var c=d.createElement('canvas');return[c,c.getContext('2d')];}
var Y=mk(),yl=Y[0],lx=Y[1];
var cos=d.createElement('div');cos.className='cos';stage.appendChild(cos);
var fp=d.getElementById('fpanel'),fpt=d.getElementById('fp-title'),fpb=d.getElementById('fp-body'),fback=d.getElementById('fp-back');
var TW=G.w,P=G.pitch,SX=G.sx,PY=G.py,HX=G.gx||8,HY=G.gy||3,Y0=G.yaw*Math.PI/180,ARC=(G.arc==null?90:G.arc)*Math.PI/180,ZOOM=G.zoom||.85;
var EDGE=G.edge==null?8:G.edge,GAP=G.gap==null?8:G.gap,MINSLOT=G.minSlot||110,FMS=G.ms||600,FILL=G.fill||.7,LGAP=G.layerGap||.8;
var yaw=Y0;
var W=0,H=0,dpr=1,K=0,LH=5,LZ=5,CX=0,CY=0,bandTop=4,bandBottom=0,CH=61,cmp=false,tiny=false;
var hover=null,hits=[],col={},mc={},cards=[],slots=[],slotKey='';
var reduce=!!(window.matchMedia&&matchMedia('(prefers-reduced-motion: reduce)').matches);
var raf=0,lastDraw=-1e9,drag=null,down=null,vis=true,GM=1;
var sceneDirty=true,rz=false,sceneYaw=NaN,ordYaw=NaN,ord=[],VF=[];
var MS=30;
// focus: fi = focused tower (-1 none), fslug its slug, fe = eased progress 0..1 of the transition (1 = focused), fanim = the transition is running
var fi=-1,fslug='',fe=0,fb=0,fdir=0,fanim=false,ft0=0,fcard=-1,cz=1,cdx=0,cdy=0,tz=1,tdx=0,tdy=0;
// the pan stays inside Y0 +/- ARC (a 180 degree arc): drag and keys go through clampYaw. Rotation is manual only.
function clampYaw(v){return Math.max(Y0-ARC,Math.min(Y0+ARC,v));}
function esc(s){return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');}
function hex(s){var m=/^#([0-9a-f]{6})$/i.exec((s||'').trim());if(!m)return[128,128,128];var n=parseInt(m[1],16);return[n>>16&255,n>>8&255,n&255];}
function mix(c,t,a){return[0,1,2].map(function(i){return Math.round(c[i]+(t[i]-c[i])*a);});}
function rgb(c){return'rgb('+c[0]+','+c[1]+','+c[2]+')';}
function rgba(c,a){return'rgba('+c[0]+','+c[1]+','+c[2]+','+a+')';}
function readColors(){var cs=getComputedStyle(d.documentElement);function v(n){return hex(cs.getPropertyValue(n));}
var ink=v('--ink');
col={ink:rgb(ink),page:rgb(v('--page')),soft:rgb(v('--soft')),good:rgb(v('--good')),bad:rgb(v('--bad')),st:{},plate:rgba(ink,.07),plateEdge:rgba(ink,.2)};
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
tiny=W<=360||(W<=540&&innerWidth<900);cmp=tiny||(W-2*EDGE)/Math.max(1,N)<MINSLOT;EX=HC&&!cmp?15:0;
CH=tiny?36:cmp?46:61+EX;bandBottom=bandTop+CH+10;
var mh=0,a,e;for(a=0;a<Math.PI;a+=Math.PI/36){e=ext(a,1,0,true);mh=Math.max(mh,Math.abs(e[0]),Math.abs(e[1]));}
var avail=H-bandBottom-18;K=(W-16)/(2*mh);
for(;;){LH=K*.9;while(LH>K*.25){e=extArc(K,LH);if(e[3]-e[2]<=avail)break;LH*=.97;}if(LH>K*.25||K<3)break;K*=.97;}
K*=ZOOM;LH*=ZOOM;e=extArc(K,LH);CX=W/2;CY=bandBottom+8+(H-6-bandBottom-8-(e[3]-e[2]))/2-e[2];
sceneDirty=true;return true;}
// pr: the projection of the resting scene; pj: the same through the focus camera (zoom cz about the centre, then a shift)
function pr(x,y,z){var c=Math.cos(sceneYaw),s=Math.sin(sceneYaw),xr=x*c-y*s,yr=x*s+y*c;return[CX+(xr-yr)*SX*K,CY+(xr+yr)*PY*K-z*LH,xr+yr];}
function pj(x,y,z){var p=pr(x,y,z);return[CX+(p[0]-CX)*cz+cdx,CY+(p[1]-CY)*cz+cdy,p[2]];}
var CORN=[[-1,-1],[1,-1],[1,1],[-1,1]],NORM=[[0,-1],[1,0],[0,1],[-1,0]];
// ---- layer: everything that depends on the yaw (the ground plate turns with it). Painter order and visible faces are cached per yaw.
function order(){if(ordYaw===sceneYaw)return;ordYaw=sceneYaw;var c=Math.cos(sceneYaw),s=Math.sin(sceneYaw);VF=[];
NORM.forEach(function(n,f){var xr=n[0]*c-n[1]*s,yr=n[0]*s+n[1]*c;if(xr+yr>.001)VF.push([f,xr-yr<0?'l':'r']);});
ord=[];var i;for(i=0;i<N;i++)ord.push({d:pj((i-(N-1)/2)*P,0,0)[2],i:i});
ord.sort(function(a,b){return a.d-b.d;});}
// a unit square (u,v) mapped onto the parallelogram (a; a+u; a+v): a face, a ground plate or a tower top is one fillRect
function tf(x,ux,uy,vx,vy,ox,oy){x.setTransform(dpr*ux,dpr*uy,dpr*vx,dpr*vy,dpr*ox,dpr*oy);}
function bbox(q){var a=1e9,b=1e9,c=-1e9,e=-1e9;q.forEach(function(p){a=Math.min(a,p[0]);b=Math.min(b,p[1]);c=Math.max(c,p[0]);e=Math.max(e,p[1]);});return[a,b,c,e];}
function path(x,q){x.beginPath();q.forEach(function(p,i){if(i)x.lineTo(p[0],p[1]);else x.moveTo(p[0],p[1]);});x.closePath();}
function drawGround(x){var q=[[-HX,-HY],[HX,-HY],[HX,HY],[-HX,HY]],gq=q.map(function(c){return pj(c[0],c[1],0);});
x.setTransform(dpr,0,0,dpr,0,0);path(x,gq);x.fillStyle=col.plate;x.fill();x.strokeStyle=col.plateEdge;x.lineWidth=1;x.lineJoin='round';x.stroke();}
// grid of one face run (z0..z1): the horizontal seams are one dashed stroke down the face, each vertical seam is a thin fillRect
function grid(x,z0,z1,colour,alpha,us,ex){var dv=.75/Math.max(.5,Math.abs(ex)),dl=.75/LZ;x.globalAlpha=alpha*GM;
if(LZ>=2.2){x.strokeStyle=colour;x.lineWidth=1;x.setLineDash([dl,1-dl]);x.beginPath();x.moveTo(.5,z0-dl/2);x.lineTo(.5,z1+dl*.6);x.stroke();x.setLineDash([]);}
x.fillStyle=colour;us.forEach(function(u){x.fillRect(u-dv/2,z0,dv,z1-z0);});x.globalAlpha=GM;}
// the layers of the focused tower stand apart by gu (in layers) once it is exploded (xp 0..1)
function gapOf(t){return Math.max(LGAP,t.height*.03);}
function drawTower(x,i,m,xp,hit){var t=T[i],L=t.layers,nl=L.length;if(!nl)return;
var x0=(i-(N-1)/2)*P,h=TW/2,z=[0],k,ns=0,gu=gapOf(t)*xp;GM=m;x.globalAlpha=m;
for(k=0;k<nl;k++){z.push(z[k]+L[k].h);if(!col.st[L[k].k].ghost&&ns===k)ns=k+1;}
function zb(k){return z[k]+k*gu;}function zt(k){return z[k+1]+k*gu;}
// the side faces of the runs k0..k1-1 and their grid: one group is one grid stroke (the seam between two runs is drawn once)
function side(k0,k1){if(k1<=k0)return;var sol=!col.st[L[k0].k].ghost;
VF.forEach(function(F){var a=CORN[F[0]],b=CORN[(F[0]+1)%4],p0=pj(x0+a[0]*h,a[1]*h,0),p1=pj(x0+b[0]*h,b[1]*h,0),ex=p1[0]-p0[0],ey=p1[1]-p0[1],kk;
tf(x,ex,ey,0,-LZ,p0[0],p0[1]);
for(kk=k0;kk<k1;kk++){var cc=col.st[L[kk].k];x.fillStyle=cc.ghost?cc.g:cc[F[1]];x.fillRect(0,zb(kk),1,L[kk].h);
if(hit){var q=[[p0[0],p0[1]-zb(kk)*LZ],[p1[0],p1[1]-zb(kk)*LZ],[p1[0],p1[1]-zt(kk)*LZ],[p0[0],p0[1]-zt(kk)*LZ]];
hits.push({q:q,bb:bbox(q),t:i,l:kk,s:F[1],m:[dpr*ex,dpr*ey,0,-dpr*LZ,dpr*p0[0],dpr*p0[1]],r:[0,zb(kk),1,L[kk].h]});}}
if(sol)grid(x,zb(k0),zt(k1-1),col.page,.8,[.5],ex);else grid(x,zb(k0),zt(k1-1),col.ink,.4,[0,.5,1],ex);});}
function top(k){var cc=col.st[L[k].k],zz=zt(k),c0=pj(x0-h,-h,zz),c1=pj(x0+h,-h,zz),c2=pj(x0+h,h,zz),c3=pj(x0-h,h,zz),q=[c0,c1,c2,c3];
tf(x,c1[0]-c0[0],c1[1]-c0[1],c3[0]-c0[0],c3[1]-c0[1],c0[0],c0[1]);x.fillStyle=cc.ghost?cc.g:cc.t;x.fillRect(0,0,1,1);
if(hit)hits.push({q:q,bb:bbox(q),t:i,l:k,s:'t',m:[dpr*(c1[0]-c0[0]),dpr*(c1[1]-c0[1]),dpr*(c3[0]-c0[0]),dpr*(c3[1]-c0[1]),dpr*c0[0],dpr*c0[1]],r:[0,0,1,1]});
if(cc.ghost){x.strokeStyle=cc.c;x.lineWidth=2/(Math.hypot(c1[0]-c0[0],c1[1]-c0[1])+Math.hypot(c3[0]-c0[0],c3[1]-c0[1]));x.strokeRect(0,0,1,1);}}
if(xp>0){for(k=0;k<nl;k++){side(k,k+1);top(k);}}
else{side(0,ns);side(ns,nl);top(nl-1);}
x.globalAlpha=1;GM=1;}
function tw(x,f,s){var k='w|'+f+'|'+s,v=mc[k];if(v!==undefined)return v;x.font=f;return mc[k]=x.measureText(s).width;}
function txt(x,s,px,py,f){x.lineWidth=3;x.lineJoin='round';x.strokeStyle=col.page;x.strokeText(s,px,py);x.fillStyle=f;x.fillText(s,px,py);}
function sh(v){return v<1000?String(v):(v/1000).toFixed(1)+'k';}
var F2='500 12px Roboto,Arial,sans-serif';
function narrow(){return W<560;}
function panelW(){return narrow()?0:Math.min(380,Math.round(W*.42));}
// callout cards in one row, ordered by the towers' x on screen (calloutLayout mirrors callout_layout in pr-scan.py)
function crosses(a,b,c,e){function o(p,q,r){return(q[0]-p[0])*(r[1]-p[1])-(q[1]-p[1])*(r[0]-p[0]);}return o(a,b,c)*o(a,b,e)<0&&o(c,e,a)*o(c,e,b)<0;}
function calloutLayout(xs,ys,pw,cy){var n=xs.length,sw=(pw-2*EDGE)/Math.max(1,n),od=[],cx=[],k,m,i,ch=true;
for(i=0;i<n;i++)od.push(i);od.sort(function(a,b){return xs[a]-xs[b]||a-b;});
for(k=0;k<n;k++)cx.push(EDGE+(k+.5)*sw);
function sg(k){return[[cx[k],cy],[xs[od[k]],ys[od[k]]]];}
while(ch){ch=false;for(k=0;k<n;k++)for(m=k+1;m<n;m++){var a=sg(k),b=sg(m);if(crosses(a[0],a[1],b[0],b[1])){var tmp=od[k];od[k]=od[m];od[m]=tmp;ch=true;}}}
return od.map(function(i,k){return{i:i,x0:EDGE+k*sw,cx:cx[k],w:sw,compact:sw<MINSLOT};});}
// card text: the longest variant that fits the card's inner width (about 6.3 px a character), as the SVG does
function ew(s){return s.length*6.3;}
function cardHtml(t,inner){var base=t.open+' open',full=' · '+t.unreviewed+' unreviewed',ab=' · '+t.unreviewed+' unrev.',us=ew(base+full)<=inner?full:ew(base+ab)<=inner?ab:'',m='';
if(HC){m=t.merged+' merged · '+t.closed+' closed ('+DAYS+')';if(ew(m)>inner)m=t.merged+' merged · '+t.closed+' closed';if(ew(m)>inner)m=t.merged+' merged';m='<span class="cd-m">'+esc(m)+'</span>';}
return '<span class="cd-nm">'+esc(t.name)+'</span><span class="cd-n"><b>'+t.open+'</b><span class="cd-w"> open</span><span class="cd-s">'+us+'</span></span>'
+'<span class="cd-d"><i class="add">+'+sh(t.add)+'</i> <i class="del">−'+sh(t['del'])+'</i></span>'+m;}
function mkCards(){cos.innerHTML='';slotKey='';
cards=T.map(function(t,i){var b=d.createElement('button'),sum=t.name+' · '+t.open+' open · '+t.unreviewed+' unreviewed'+(HC?' · '+t.merged+' merged · '+t.closed+' closed ('+DAYS+')':'');
b.type='button';b.className='cd';b.setAttribute('data-i',String(i));b.setAttribute('data-tip',sum);b.setAttribute('aria-label','Focus '+sum);
b.innerHTML=cardHtml(t,1e9);
b.addEventListener('click',function(){fcard=i;enter(i);});cos.appendChild(b);return b;});}
// the cards stay in their slots while the yaw changes; when the on-screen order changes they slide to the new slots (CSS transform ease)
function placeCards(){var geo=W+'|'+CH+'|'+(cmp?1:0)+(tiny?1:0)+'|'+N,key=slots.map(function(s){return s.i;}).join(',')+'|'+geo;
if(key===slotKey)return;var same=slotKey.slice(slotKey.indexOf('|')+1)===geo;slotKey=key;
cos.className='cos'+(tiny?' t':cmp?' c':'');
if(!same&&slots.length){var inner=slots[0].w-GAP-2*10;cards.forEach(function(b,i){if(T[i])b.innerHTML=cardHtml(T[i],inner);});}
slots.forEach(function(s){var b=cards[s.i];if(!b)return;b.style.transition=same&&!reduce?'transform .28s ease-out':'none';
b.style.width=Math.max(0,s.w-GAP)+'px';b.style.height=CH+'px';b.style.transform='translate('+(s.x0+GAP/2)+'px,'+bandTop+'px)';});
if(!(d.activeElement&&d.activeElement.parentNode===cos))slots.forEach(function(s){if(cards[s.i])cos.appendChild(cards[s.i]);});}
// leader lines: card bottom centre to the tower top. Drawn in the layer; they fade out while a tower is focused.
function drawLeaders(x){if(fe>=1||!N)return;x.setTransform(dpr,0,0,dpr,0,0);x.globalAlpha=1-fe;x.beginPath();
slots.forEach(function(s){var t=T[s.i],p=pj((s.i-(N-1)/2)*P,0,t.height);x.moveTo(s.cx,bandTop+CH);x.lineTo(p[0],p[1]-3);});
x.strokeStyle=col.ink;x.lineWidth=1;x.stroke();x.globalAlpha=1;}
// the focused tower: a label beside each layer once the layers stand apart
function sideLabels(x){var t=T[fi],L=t.layers,x0=(fi-(N-1)/2)*P,h=TW/2,gu=gapOf(t)*fe,z=0,lim=W-panelW()-8,k;
x.setTransform(dpr,0,0,dpr,0,0);x.textBaseline='alphabetic';x.globalAlpha=Math.max(0,(fe-.5)*2);
for(k=0;k<L.length;k++){var zm=z+k*gu+L[k].h/2,ps=CORN.map(function(c){return pj(x0+c[0]*h,c[1]*h,zm);}),xr=-1e9,xl=1e9,yc=pj(x0,0,zm)[1],s=L[k].label+' · '+L[k].n,w=tw(x,F2,s),lx0;
ps.forEach(function(p){xr=Math.max(xr,p[0]);xl=Math.min(xl,p[0]);});z+=L[k].h;
x.strokeStyle=col.st[L[k].k].c;x.lineWidth=2;x.beginPath();
if(xr+16+w>lim){x.moveTo(xl-2,yc);x.lineTo(xl-10,yc);lx0=xl-14-w;}else{x.moveTo(xr+2,yc);x.lineTo(xr+10,yc);lx0=xr+14;}
x.stroke();x.font=F2;txt(x,s,Math.max(4,lx0),yc+4,col.ink);}
x.globalAlpha=1;}
// the camera of the focused tower at the end of the transition: it fills FILL of the stage height, centred left of the panel
function camTarget(){var t=T[fi],x0=(fi-(N-1)/2)*P,h=TW/2,top=t.height+(t.layers.length-1)*gapOf(t),b=[1e9,-1e9,1e9,-1e9],nw=narrow(),
rw=W-panelW(),rh=nw?H*.42:H,Z;
[[-h,-h],[h,-h],[h,h],[-h,h]].forEach(function(q){[0,top].forEach(function(z){var p=pr(x0+q[0],q[1],z);b[0]=Math.min(b[0],p[0]);b[1]=Math.max(b[1],p[0]);b[2]=Math.min(b[2],p[1]);b[3]=Math.max(b[3],p[1]);});});
Z=Math.max(1,Math.min(4,FILL*rh/(b[3]-b[2]),.5*rw/(b[1]-b[0])));
tz=Z;tdx=rw/2-(CX+((b[0]+b[1])/2-CX)*Z);tdy=rh/2-(CY+((b[2]+b[3])/2-CY)*Z);}
function buildScene(){sceneDirty=false;sceneYaw=yaw;hits=[];order();
if(fi>=0)camTarget();else{tz=1;tdx=0;tdy=0;}
cz=1+(tz-1)*fe;cdx=tdx*fe;cdy=tdy*fe;LZ=LH*cz;
var tx=[],ty=[],i;for(i=0;i<N;i++){var p=pr((i-(N-1)/2)*P,0,T[i].height);tx.push(p[0]);ty.push(p[1]-3);}
slots=calloutLayout(tx,ty,W,bandTop+CH);placeCards();
var x=lx;x.setTransform(1,0,0,1,0,0);x.clearRect(0,0,yl.width,yl.height);
x.globalAlpha=1-.65*fe;drawGround(x);x.globalAlpha=1;
ord.forEach(function(it){var f=it.i===fi;drawTower(x,it.i,f?1:1-.86*fe,f?fe:0,fi<0||f);});
if(fi>=0&&fe>.5)sideLabels(x);
drawLeaders(x);x.setTransform(1,0,0,1,0,0);x.globalAlpha=1;}
// ---- composite: the only thing a normal frame does. The canvas is cleared, so the page gradient shows through.
function composite(){g.setTransform(1,0,0,1,0,0);g.clearRect(0,0,cv.width,cv.height);g.drawImage(yl,0,0);
if(hover){hits.forEach(function(h){if(h.t!==hover.t||h.l!==hover.l)return;var cc=col.st[T[h.t].layers[h.l].k];
g.setTransform(h.m[0],h.m[1],h.m[2],h.m[3],h.m[4],h.m[5]);g.fillStyle=cc.ghost?cc.gh:cc['h'+h.s];g.fillRect(h.r[0],h.r[1],h.r[2],h.r[3]);});g.setTransform(1,0,0,1,0,0);}}
function shown(){return vis&&!d.hidden&&N>0;}
// the loop runs only while the focus transition plays (a pan or a hover draws one frame and stops); at rest nothing is scheduled
function sched(){if(!raf&&shown())raf=requestAnimationFrame(frame);}
function eo(u){return 1-Math.pow(1-u,3);}
function frame(ts){raf=0;
if(!shown())return;
if(ts-lastDraw<MS){raf=requestAnimationFrame(frame);return;}
lastDraw=ts;
if(rz){rz=false;layout();}
if(fanim){if(!ft0)ft0=ts;var u=Math.min(1,(ts-ft0)/FMS),tg=fdir>0?1:0;fe=fb+(tg-fb)*eo(u);sceneDirty=true;
if(u>=1){fanim=false;ft0=0;fe=tg;if(!tg)finishLeave();}}
if(sceneDirty||sceneYaw!==yaw)buildScene();
composite();
if(fanim)sched();}
// ---- focus
function idx(s){for(var i=0;i<N;i++)if(T[i].slug===s)return i;return -1;}
function setHash(s){try{d.dispatchEvent(new CustomEvent('prscan-focus-set',{detail:s}));}catch(e){}}
function panelHtml(t){var o=[],L=t.layers.slice().reverse(),k;
if(t.members&&t.members.length)o.push('<p class="fp-mem">'+esc(t.members.length+' areas: '+t.members.join(', '))+'</p>');
o.push('<ul class="fp-counts">');
L.forEach(function(l){o.push('<li><i class="fp-sw v-'+esc(l.k)+'"></i><span>'+esc(l.label)+'</span><b>'+l.n+'</b></li>');});
if(HC)o.push('<li><i class="fp-sw"></i><span>Closed, not merged</span><b>'+t.closed+'</b></li>');
o.push('</ul>');
if(t.files&&t.files.length){o.push('<h4>Top files</h4><ul class="fp-files">');t.files.forEach(function(f){o.push('<li>'+esc(f[0])+(f[1]>1?' ×'+f[1]:'')+'</li>');});o.push('</ul>');}
o.push('<div class="fp-prs">');
L.forEach(function(l){var ns=(t.prs&&t.prs[l.k])||[];if(!ns.length)return;o.push('<h4>'+esc(l.label)+' · '+ns.length+'</h4><ol>');
ns.forEach(function(n){var e=TL[n]||['','#',''];o.push('<li><a href="'+e[1]+'">#'+n+'</a> '+e[0]+'</li>');});o.push('</ol>');});
if(!L.length)o.push('<p class="fp-mem">No PRs.</p>');
o.push('</div>');return o.join('');}
function fillPanel(){if(fi<0||!fp)return;if(fpt)fpt.textContent=T[fi].name;if(fpb)fpb.innerHTML=panelHtml(T[fi]);}
function enter(i,inst){if(i<0||i>=N)return;fi=i;fslug=T[i].slug;stage.classList.add('foc');hover=null;if(tip)tip.hidden=true;fillPanel();
if(fp){fp.hidden=false;fp.offsetWidth;fp.classList.add('on');if(fback&&!inst&&fback.focus)fback.focus();}
if(reduce||inst||!K){fe=1;fanim=false;}else{fb=fe;fdir=1;fanim=true;ft0=0;}
setHash(fslug);sceneDirty=true;sched();}
function finishLeave(){fi=-1;fslug='';sceneDirty=true;if(fp&&!fp.classList.contains('on'))fp.hidden=true;}
function leave(inst){if(fi<0)return;setHash('');stage.classList.remove('foc');if(fp)fp.classList.remove('on');
var c=cards[fcard];fcard=-1;if(c&&c.focus)c.focus();
if(reduce||inst||!K){fe=0;fanim=false;finishLeave();}else{fb=fe;fdir=-1;fanim=true;ft0=0;}
sceneDirty=true;sched();}
if(fback)fback.addEventListener('click',function(){leave();});
d.addEventListener('keydown',function(e){if(e.key==='Escape'&&fi>=0){e.preventDefault();leave();}});
d.addEventListener('prscan-focus',function(e){var s=e.detail||'',j=s?idx(s):-1;
if(!s){leave();return;}if(j<0||j===fi)return;layout();
if(fi>=0){fi=j;fslug=s;fillPanel();sceneDirty=true;sched();}else enter(j);});
// ---- pointer and keys
function pos(e){var r=cv.getBoundingClientRect();return[e.clientX-r.left,e.clientY-r.top];}
function place(e){var x=e.clientX+12,y=e.clientY+14,w=tip.offsetWidth,h=tip.offsetHeight;
if(x+w>innerWidth-8)x=Math.max(8,e.clientX-w-12);if(y+h>innerHeight-8)y=Math.max(8,e.clientY-h-12);tip.style.left=x+'px';tip.style.top=y+'px';}
function inQ(q,px,py){var s=0;for(var i=0;i<q.length;i++){var a=q[i],b=q[(i+1)%q.length],c=(b[0]-a[0])*(py-a[1])-(b[1]-a[1])*(px-a[0]);
if(c!==0){if(s&&(c>0)!==(s>0))return false;s=c;}}return true;}
// hit-test on the polygons cached when the layer was built; the canvas is repainted only when the hovered layer changes
function hitAt(e){var p=pos(e),i;
for(i=hits.length-1;i>=0;i--){var h=hits[i],b=h.bb;if(p[0]<b[0]||p[0]>b[2]||p[1]<b[1]||p[1]>b[3])continue;if(inQ(h.q,p[0],p[1]))return h;}
return null;}
function pick(e){var f=hitAt(e),ch=(f?f.t:-1)!==(hover?hover.t:-1)||(f?f.l:-1)!==(hover?hover.l:-1);hover=f?{t:f.t,l:f.l}:null;
if(f&&tip){var t=T[f.t],ly=t.layers[f.l];tip.textContent=t.name+' · '+ly.label.toLowerCase()+' · '+ly.n+' PR'+(ly.n===1?'':'s');tip.hidden=false;place(e);}
else if(tip)tip.hidden=true;if(ch)sched();}
cv.addEventListener('pointerdown',function(e){drag={x:e.clientX};down={x:e.clientX,y:e.clientY,m:0};if(cv.setPointerCapture)try{cv.setPointerCapture(e.pointerId);}catch(x){}});
cv.addEventListener('pointermove',function(e){if(drag){down.m=Math.max(down.m,Math.abs(e.clientX-down.x)+Math.abs(e.clientY-down.y));
if(down.m>3){yaw=clampYaw(yaw+(e.clientX-drag.x)*.01);hover=null;if(tip)tip.hidden=true;sched();}drag.x=e.clientX;}else pick(e);});
// a press that did not move is a click: a tower focuses it, empty stage leaves the focus
cv.addEventListener('pointerup',function(e){var c=down&&down.m<=3;drag=null;down=null;
if(c){var f=hitAt(e);if(f&&fi<0){fcard=-1;enter(f.t);}else if(fi>=0&&!(f&&f.t===fi))leave();}sched();});
cv.addEventListener('pointercancel',function(){drag=null;down=null;sched();});
cv.addEventListener('pointerleave',function(){hover=null;if(tip)tip.hidden=true;sched();});
cv.addEventListener('keydown',function(e){var k=e.key==='ArrowLeft'?-1:e.key==='ArrowRight'?1:0;if(!k)return;e.preventDefault();yaw=clampYaw(yaw+k*.12);sched();});
// the size is re-read in the next frame, so a burst of resize events costs one layout
function resized(){rz=true;sched();}
if(window.ResizeObserver)new ResizeObserver(resized).observe(stage);else addEventListener('resize',resized);
if(window.IntersectionObserver)new IntersectionObserver(function(es){vis=es[es.length-1].isIntersecting;if(vis)sched();}).observe(cv);
d.addEventListener('visibilitychange',function(){if(!d.hidden)sched();});
function recolor(){readColors();sceneDirty=true;sched();}
if(window.MutationObserver)new MutationObserver(recolor).observe(d.documentElement,{attributes:true,attributeFilter:['data-theme']});
if(window.matchMedia)try{matchMedia('(prefers-color-scheme: dark)').addEventListener('change',recolor);}catch(e){}
if(d.fonts&&d.fonts.ready)d.fonts.ready.then(function(){mc={};sceneDirty=true;sched();});
// a range change swaps the towers: the layout is recomputed (plate and cards depend on them); a focused tower stays focused when the new range has it
var semp=d.getElementById('scene-empty'),skey=d.querySelector('.scene-key');
function setRange(k){var R=RG[k];if(!R)return;T=R.towers||[];N=T.length;HX=R.gx||8;HY=R.gy||3;DAYS=R.rl||DAYS;HC=!!R.hasClosed;
hover=null;hits=[];if(tip)tip.hidden=true;stage.hidden=!N;mkCards();
if(semp){semp.textContent='No PRs in the last '+DAYS+'.';semp.hidden=!!N;}
if(skey)skey.innerHTML=R.key||'';
if(fi>=0){var j=idx(fslug);if(j<0){leave(true);}else{fi=j;fillPanel();}}
K=0;sceneDirty=true;if(N)layout();sched();}
d.addEventListener('prscan-range',function(e){setRange(e.detail);});
readColors();var r0=d.documentElement.getAttribute('data-range');
if(r0&&RG[r0])setRange(r0);else{mkCards();layout();sched();}
var f0=d.documentElement.getAttribute('data-focus'),j0=f0?idx(f0):-1;if(j0>=0)enter(j0,true);
})();
"""


def top_files(prs, n=3):
    """The n files most PRs touch, as [(path, PRs touching it, changed lines)]; ties: more changed lines, then path."""
    cnt, lines = {}, {}
    for p in prs:
        for f in p.get("files", []):
            cnt[f["path"]] = cnt.get(f["path"], 0) + 1
            lines[f["path"]] = lines.get(f["path"], 0) + f.get("additions", 0) + f.get("deletions", 0)
    return sorted(((f, cnt[f], lines[f]) for f in cnt), key=lambda t: (-t[1], -t[2], t[0]))[:n]


def merged_per_day(closed, now, days=TREND_DAYS):
    """PRs merged per UTC day over the last `days` days (today last, zeros included), from the closed PRs; [] when they
    are unknown (closed is None)."""
    if closed is None:
        return []
    start = now.date() - dt.timedelta(days=days - 1)
    counts = {}
    for p in closed:
        if p.get("mergedAt"):
            d = parse_time(p["mergedAt"]).date()
            counts[d] = counts.get(d, 0) + 1
    out = []
    for i in range(days):
        d = start + dt.timedelta(days=i)
        out.append({"day": d.isoformat(), "merged": counts.get(d, 0)})
    return out


def ts_attrs(p):
    """data-created / data-updated (open PRs) or data-closed (closed and merged PRs), in epoch seconds: the JS filters
    list rows by the time range with these."""
    def e(k):
        return int(parse_time(p[k]).timestamp()) if p.get(k) else 0
    if p.get("state", "OPEN") == "OPEN":
        return 'data-created="%d" data-updated="%d"' % (e("createdAt"), e("updatedAt"))
    return 'data-closed="%d"' % (int(parse_time(closed_time(p)).timestamp()) if closed_time(p) else 0)


def standings_html(view, rl, spark_hist):
    """The rows of the Areas list for one range view: rank, area (group), state mix bar, open count, 7-day sparkline, merged and
    closed counts of the range, and the top 3 changed files. The empty-range message when there is nothing."""
    top = top_areas(view)
    has_closed = view["closed"] is not None
    if not top:
        return '<li class="empty-block">%s</li>' % esc(empty_range(rl))
    biggest = max([len(e["prs"]) + len(e["merged"]) for e in top] + [1])
    o = []
    for i, e in enumerate(top, 1):
        a, ps, members = e["name"], e["prs"], e["members"]
        cnt = {k: sum(bucket(p) == k for p in ps) for k, _ in BUCKETS}
        nm = len(e["merged"])
        segs = "".join('<i class="%s" style="flex:%d" data-tip="%s" title="%s"></i>' % (
            k, c, esc("%s: %d" % (n, c)), esc("%s: %d" % (n, c))) for k, n, c in
            [(k, n, cnt[k]) for k, n in BUCKETS] + [("merged", "Merged in " + rl, nm)] if c)
        ma = members or [a]
        series = downsample([sum(s["areas"].get(m, 0) for m in ma) for s in spark_hist if s["open"] == 0 or s["areas"]], 48)
        who = (ptype(a, link=True) if not members else '<span class="ptype" style="--ac:var(--soft)" title="%s"><i></i><span class="nm">%s</span></span>' % (
            esc(", ".join(area_label(m) for m in members)), esc(a)))
        sub = (" · ".join("%d %s" % (cnt[k], n.lower()) for k, n in BUCKETS if cnt[k]) if not members else
               "%d area%s: %s" % (len(members), "" if len(members) == 1 else "s", ", ".join(area_label(m) for m in members)))
        if has_closed:
            sub += (" · " if sub else "") + "%d merged · %d closed" % (nm, len(e["closed"]))
        files = top_files(ps)
        chips = "".join('<span class="fchip" title="%s">%s</span>' % (
            esc("%s: %d PR%s, %d changed lines" % (f, n, "" if n == 1 else "s", ln)), esc(f + (" ×%d" % n if n > 1 else "")))
            for f, n, ln in files)
        o.append('<li class="st" data-slug="%s"><span class="rank">%d</span><div class="who"><b>%s</b><small title="%s">%s</small></div>'
                 '<span class="track" role="button" tabindex="0" aria-label="Focus %s on the effort map"><span class="fill" style="width:%.1f%%">%s</span></span>'
                 '<div class="where"><b>%d</b>%s</div>%s</li>' % (
                     esc(entry_slug(e)), i, who, esc(sub), esc(sub), esc(a),
                     100.0 * (len(ps) + nm) / biggest, segs, len(ps),
                     sparkline(series, "%s: open PRs over %d days" % (a, SPARK_DAYS)),
                     '<div class="stf">%s</div>' % chips if chips else ""))
    return "".join(o)


def select_counts(top, view):
    """Open PR counts of one range view for every option of the Area select, keyed by option value. top: the entries the
    select is built from (the page's full top_areas), so the keys are the same in every range."""
    n = lambda a: len(view["areas"].get(a, []))
    out = {}
    for e in top:
        if not e["members"]:
            out[slug(e["name"])] = n(e["name"])
            continue
        out[entry_slug(e)] = sum(n(m) for m in e["members"])
        for m in e["members"]:
            out[slug(m)] = n(m)
    return out


FOCUS_PANEL = ('<aside class="fpanel" id="fpanel" role="region" aria-label="Area detail" hidden><div class="fp-head">'
               '<button type="button" class="fp-back" id="fp-back">Back</button><h3 id="fp-title"></h3></div><div class="fp-body" id="fp-body"></div></aside>')

# ---------------------------------------------------------------- Progress page: ladder math, README / HANDOFF parsing

PROGRESS_FILE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "progress.json")
GAME_STEPS = ("loads", "boots", "menus", "ingame", "playable")
LADDER_KEYS = ("groundwork",) + GAME_STEPS
ASSESSORS = ("manual", "claude-routine")
RELATED_JACCARD = 0.2    # a PR title this similar to a next step (or sharing a source path with it) is related to it
RELATED_MAX = 5
COMMITS_SHOWN = 30
COMMIT_PAGES = 3         # pages of 100 commits fetched for the longest range
AREAS_SHOWN = 5
SHORT_MAX = 150
NO_DATA = "No data yet"


def _clamp01(v):
    try:
        return min(1.0, max(0.0, float(v)))
    except (TypeError, ValueError):
        return 0.0


def _ladder_sorted(ladder):
    return sorted(ladder, key=lambda s: s["pct"])


def game_reach(levels, ladder):
    """(furthest reached step, next step or None, fraction 0..1 of the next one reached) for a game's levels. Groundwork is
    always reached; a step is reached at level 1; the furthest one counts even if an earlier one is partial."""
    steps = _ladder_sorted(ladder)
    cur = 0
    for i, s in enumerate(steps[1:], 1):
        if _clamp01(levels.get(s["key"], 0)) >= 1:
            cur = i
    nxt = steps[cur + 1] if cur + 1 < len(steps) else None
    return steps[cur], nxt, (_clamp01(levels.get(nxt["key"], 0)) if nxt else 0.0)


def game_pct(levels, ladder):
    """A game's %: the position of its furthest reached step plus the partial fraction toward the next step."""
    cur, nxt, frac = game_reach(levels, ladder)
    return round(cur["pct"] + (frac * (nxt["pct"] - cur["pct"]) if nxt else 0.0), 1)


def is_playable(game):
    return _clamp01(game["levels"].get("playable", 0)) >= 1


def overall_progress(games, ladder):
    """{"pct", "step", "game", "playable", "total"}: the % of the furthest game not yet Playable (its step label and name), 100 and the
    last step when every game is playable; None without games. "playable" of "total" games are Playable."""
    if not games:
        return None
    steps = _ladder_sorted(ladder)
    open_games = [g for g in games if not is_playable(g)]
    n = len(games) - len(open_games)
    if not open_games:
        return {"pct": 100.0, "step": steps[-1]["label"], "game": None, "playable": n, "total": len(games)}
    best = max(open_games, key=lambda g: game_pct(g["levels"], ladder))
    return {"pct": game_pct(best["levels"], ladder), "step": game_reach(best["levels"], ladder)[0]["label"], "game": best["name"],
            "playable": n, "total": len(games)}


def ladder_markers(games, ladder):
    """One marker per game on the mega ladder, [{"name", "pct", "row", "playable"}]; games at the same % stack in rows (0 = on the bar)."""
    out, seen = [], {}
    for g in sorted(games, key=lambda g: (game_pct(g["levels"], ladder), g["name"])):
        p = game_pct(g["levels"], ladder)
        out.append({"name": g["name"], "pct": p, "row": seen.get(p, 0), "playable": is_playable(g)})
        seen[p] = seen.get(p, 0) + 1
    return out


def fmt_pct(p):
    return str(int(p)) if float(p) == int(p) else ("%.1f" % p)


def validate_progress(d):
    """The problems of a progress.json value as a list of strings (empty when it is valid)."""
    err = []
    if not isinstance(d, dict):
        return ["not an object"]
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", str(d.get("assessedAt", ""))):
        err.append("assessedAt must be YYYY-MM-DD")
    if d.get("assessedBy") not in ASSESSORS:
        err.append("assessedBy must be one of %s" % ", ".join(ASSESSORS))
    for k in ("readmeBlob", "handoffBlob"):
        if not re.fullmatch(r"[0-9a-f]{40}", str(d.get(k, ""))):
            err.append("%s must be a 40-hex git blob sha" % k)
    ladder = d.get("ladder")
    if not isinstance(ladder, list) or [s.get("key") if isinstance(s, dict) else None for s in ladder] != list(LADDER_KEYS):
        err.append("ladder keys must be %s" % ", ".join(LADDER_KEYS))
    else:
        pcts = [s.get("pct") for s in ladder]
        if not all(isinstance(p, (int, float)) for p in pcts) or pcts != sorted(set(pcts)) or pcts[-1] != 100:
            err.append("ladder pct must increase and end at 100")
        for s in ladder:
            if not isinstance(s.get("label"), str) or not isinstance(s.get("description"), str):
                err.append("ladder step %s needs label and description" % s.get("key"))
    games = d.get("games")
    if not isinstance(games, list) or not games:
        err.append("games must be a non-empty list")
        return err
    for g in games:
        name = g.get("name") if isinstance(g, dict) else None
        if not isinstance(g, dict) or not isinstance(name, str) or not name:
            err.append("every game needs a name")
            continue
        for k in ("package", "limits", "evidence", "source"):
            if not isinstance(g.get(k), str):
                err.append("%s: %s must be a string" % (name, k))
        lv = g.get("levels")
        if not isinstance(lv, dict) or sorted(lv) != sorted(GAME_STEPS) or not all(
                isinstance(v, (int, float)) and not isinstance(v, bool) and 0 <= v <= 1 for v in lv.values()):
            err.append("%s: levels must hold %s as numbers 0..1" % (name, ", ".join(GAME_STEPS)))
    return err


def load_progress(path=PROGRESS_FILE):
    """progress.json as a dict, or None when it is missing or invalid (the page then says "No data yet")."""
    try:
        with open(path, encoding="utf-8") as f:
            d = json.load(f)
    except (OSError, ValueError) as e:
        print("pr-scan: progress file unreadable: %s" % e, file=sys.stderr)
        return None
    err = validate_progress(d)
    if err:
        print("pr-scan: progress file invalid: %s" % "; ".join(err), file=sys.stderr)
        return None
    return d


# -- markdown, as plain text only: nothing is rendered as HTML except `code`, and everything is escaped first

def plain_md(s):
    """A markdown snippet as plain text: links become their text, emphasis markers go; backticks stay (inline_html)."""
    s = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", str(s))
    s = re.sub(r"<(https?://[^>\s]+)>", r"\1", s)
    s = re.sub(r"\*\*([^*]+)\*\*", r"\1", s)
    return re.sub(r"\s+", " ", s).strip()


def inline_html(s):
    """Plain text of a markdown snippet, HTML-escaped; `inline code` becomes <code>."""
    return re.sub(r"`([^`]+)`", r"<code>\1</code>", esc(plain_md(s)))


def md_cells(line):
    """The cells of a markdown table row, or None for a line that is not one. `|` inside backticks or after a backslash stays in the cell."""
    s = line.strip()
    if not s.startswith("|"):
        return None
    cells, cur, tick, i = [], [], False, 1
    while i < len(s):
        c = s[i]
        if c == "\\" and i + 1 < len(s) and s[i + 1] == "|":
            cur.append("|")
            i += 2
            continue
        if c == "`":
            tick = not tick
        if c == "|" and not tick:
            cells.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
        i += 1
    if "".join(cur).strip():
        cells.append("".join(cur).strip())
    return cells


def md_section(text, title, level=2):
    """The lines of the section whose heading starts with `title` (a heading of `level` #), up to the next heading of the same or a
    higher level; fenced code is skipped. [] when there is none."""
    out, inside, fence = [], False, False
    for line in str(text).splitlines():
        if line.lstrip().startswith("```"):
            fence = not fence
            if inside:
                out.append(line)
            continue
        m = None if fence else re.match(r"^(#{1,6})\s+(.*?)\s*#*\s*$", line)
        if m and len(m.group(1)) <= level:
            if inside:
                break
            inside = len(m.group(1)) == level and m.group(2).lower().startswith(title.lower())
            continue
        if inside:
            out.append(line)
    return out


def md_tables(lines):
    """[(nearest ### heading or "", header cells, [row cells])] for every table in the lines."""
    out, heading, i = [], "", 0
    while i < len(lines):
        m = re.match(r"^#{3,6}\s+(.*?)\s*$", lines[i])
        if m:
            heading = plain_md(m.group(1))
        h = md_cells(lines[i])
        sep = md_cells(lines[i + 1]) if h and i + 1 < len(lines) else None
        if h and sep and all(re.fullmatch(r":?-+:?", c) for c in sep):
            rows, i = [], i + 2
            while i < len(lines) and md_cells(lines[i]) is not None:
                rows.append(md_cells(lines[i]))
                i += 1
            out.append((heading, h, rows))
            continue
        i += 1
    return out


def _col(header, *names):
    low = [h.lower() for h in header]
    for n in names:
        for i, h in enumerate(low):
            if h == n or h.startswith(n):
                return i
    return None


def state_kind(cell):
    return "done" if "✅" in cell else "wip" if "\U0001f6a7" in cell else "other"


def parse_features(text):
    """README "What works": every table row with a State column, grouped by its ### heading, with % done.
    {"sections": [{"name", "rows": [{"feature", "state", "kind", "what"}], "done", "total", "pct"}], "done", "total", "pct"}; None without rows."""
    sections = []
    for heading, header, rows in md_tables(md_section(text, "What works")):
        si = _col(header, "state")
        if si is None:
            continue
        wi = _col(header, "what")
        items = []
        for r in rows:
            if len(r) <= si or not r[si].strip():
                continue
            items.append({"feature": plain_md(r[0]), "state": plain_md(r[si]), "kind": state_kind(r[si]),
                          "what": plain_md(r[wi]) if wi is not None and wi < len(r) else ""})
        if items:
            done = sum(x["kind"] == "done" for x in items)
            sections.append({"name": heading or "What works", "rows": items, "done": done, "total": len(items),
                             "pct": round(100 * done / len(items))})
    if not sections:
        return None
    done, total = sum(s["done"] for s in sections), sum(s["total"] for s in sections)
    return {"sections": sections, "done": done, "total": total, "pct": round(100 * done / total)}


def parse_games(text):
    """README "Games" table as [{"name", "package", "state", "kind", "what", "limits"}] ([] when absent); an en dash limit is empty."""
    out = []
    for _, header, rows in md_tables(md_section(text, "Games")):
        gi, si = _col(header, "game"), _col(header, "state")
        if gi is None or si is None:
            continue
        pi, wi, li = _col(header, "package"), _col(header, "what"), _col(header, "limits")
        get = lambda r, i: plain_md(r[i]) if i is not None and i < len(r) else ""
        for r in rows:
            clean = lambda s: "" if s in ("–", "-", "—") else s
            out.append({"name": get(r, gi), "package": clean(get(r, pi)), "state": get(r, si), "kind": state_kind(r[si]) if si < len(r) else "other",
                        "what": get(r, wi), "limits": clean(get(r, li))})
    return [g for g in out if g["name"]]


def parse_highlights(text):
    """README "Highlights" bullets as plain-text strings (continuation lines joined)."""
    out = []
    for line in md_section(text, "Highlights"):
        m = re.match(r"^\s*[-*]\s+(.*)$", line)
        if m:
            out.append(m.group(1))
        elif out and line.strip() and line[:1].isspace():
            out[-1] += " " + line.strip()
    return [plain_md(x) for x in out]


def first_sentence(text, limit=SHORT_MAX):
    """The text up to its first sentence end (a full stop followed by a space, outside backticks and brackets, not after e.g. / i.e.), at most `limit` characters."""
    depth, tick = 0, False
    for i, c in enumerate(text):
        if c == "`":
            tick = not tick
        elif not tick and c in "([":
            depth += 1
        elif not tick and c in ")]":
            depth = max(0, depth - 1)
        elif c == "." and not tick and not depth and (i + 1 == len(text) or text[i + 1] == " "):
            if not re.search(r"(?:\be\.g|\bi\.e|\betc|\bvs)$", text[:i]):
                text = text[:i + 1]
                break
    if len(text) <= limit:
        return text
    cut = text[:limit - 1].rstrip()
    if cut.count("`") % 2:
        cut = cut[:cut.rindex("`")].rstrip()  # never end inside an inline-code span
    return cut + "…"


def parse_next_steps(text):
    """HANDOFF "Next steps (priority)": the numbered items as [{"n", "text", "short"}] (continuation lines joined, short = first sentence)."""
    items = []
    for line in md_section(text, "Next steps"):
        m = re.match(r"^(\d+)\.\s+(.*)$", line)
        if m:
            items.append({"n": int(m.group(1)), "text": m.group(2).strip()})
        elif items and line.strip() and line[:1].isspace():
            items[-1]["text"] += " " + line.strip()
    return [{"n": x["n"], "text": x["text"], "short": first_sentence(plain_md(x["text"]))} for x in items]


def parse_in_progress(text):
    """HANDOFF "In progress (unfinished)": [{"heading", "line"}], one per sub-heading with its first line; a section without
    sub-headings gives one item with an empty heading and its first line; [] when the section is empty or absent."""
    items, cur = [], None
    for line in md_section(text, "In progress"):
        m = re.match(r"^#{3,6}\s+(.*?)\s*$", line)
        if m:
            cur = {"heading": plain_md(m.group(1)), "line": ""}
            items.append(cur)
        elif line.strip():
            if cur is None:
                cur = {"heading": "", "line": ""}
                items.append(cur)
            if not cur["line"]:
                cur["line"] = plain_md(line)
    return items


def module_paths(text):
    """The source paths a next step names, as ([exact files], [directory prefixes]): `gpu::texture` is crates/xcl/src/gpu/texture.rs or
    anything under crates/xcl/src/gpu/texture/; a literal `dir/file.rs` matches by suffix."""
    exact, spans = set(), re.findall(r"`([^`]+)`", text)
    mods = set()
    for sp in spans:
        for m in re.finditer(r"(?<![\w:])([a-z_][a-z0-9_]*(?:::[a-z_][a-z0-9_]*)+)(?![\w:])", sp):
            mods.add("crates/xcl/src/" + m.group(1).replace("::", "/"))
        for m in re.finditer(r"(?<![\w/.-])((?:[\w.-]+/)*[\w.-]+\.rs)(?![\w])", sp):
            exact.add(m.group(1))
    return exact, mods


def related_prs(item, prs, jt=RELATED_JACCARD, limit=RELATED_MAX):
    """Open PRs related to a next step: the title-word Jaccard against the step's first sentence is at least `jt`, or a changed file is a
    source path the step names. Ranked by shared paths, then similarity, then newest; up to `limit` of
    [{"number", "title", "url", "jaccard", "paths"}]."""
    toks = title_tokens(item["short"])
    lits, mods = module_paths(item["text"])
    out = []
    for p in prs:
        j = jaccard(title_tokens(p["title"]), toks)
        shared = sorted({f["path"] for f in p.get("files", []) if any(f["path"] == m + ".rs" or f["path"].startswith(m + "/") for m in mods)
                         or any(f["path"] == x or f["path"].endswith("/" + x) for x in lits)})
        if j >= jt or shared:
            out.append({"number": p["number"], "title": p["title"], "url": p["url"], "jaccard": round(j, 2), "paths": shared})
    out.sort(key=lambda r: (-len(r["paths"]), -r["jaccard"], -r["number"]))
    return out[:limit]


def parse_commits(arr):
    """The commit list of the REST API as [{"sha", "subject", "author", "ts", "url"}], newest first as given; broken entries are skipped."""
    out = []
    for c in arr if isinstance(arr, list) else []:
        try:
            cm = c["commit"]
            out.append({"sha": str(c["sha"])[:7], "subject": (cm.get("message") or "").split("\n", 1)[0].strip(),
                        "author": (c.get("author") or {}).get("login") or (cm.get("author") or {}).get("name") or "unknown",
                        "ts": (cm.get("committer") or cm.get("author") or {})["date"], "url": c.get("html_url") or ""})
        except (KeyError, TypeError, AttributeError):
            continue
    return out


def commits_in_range(commits, range_s, now):
    since = now - dt.timedelta(seconds=range_s)
    return [c for c in commits if parse_time(c["ts"]) >= since]


def active_areas(view, n=AREAS_SHOWN):
    """The n areas with the most PR activity in a range view (open PRs active in it plus closed or merged ones that ended in it), as
    [(area, count)], ties by name."""
    cnt = {}
    for p in view["prs"] + (view.get("closed") or []):
        cnt[p["primary"]] = cnt.get(p["primary"], 0) + 1
    return sorted(cnt.items(), key=lambda kv: (-kv[1], kv[0]))[:n]


def changed_docs(assess, live):
    """["README", "HANDOFF"] minus the files whose live blob sha equals the assessed one or is unknown (None)."""
    if not assess:
        return []
    return [n for n, k in (("README", "readmeBlob"), ("HANDOFF", "handoffBlob")) if live.get(n) and live[n] != assess.get(k)]


def decode_contents(obj):
    """(blob sha, text or None) from a contents-API JSON object (base64 content)."""
    import base64
    sha = obj.get("sha") if isinstance(obj, dict) else None
    if isinstance(obj, dict) and obj.get("encoding") == "base64" and obj.get("content"):
        return sha, base64.b64decode(obj["content"]).decode("utf-8", "replace")
    return sha, None


def gh_json(path):
    r = subprocess.run(["gh", "api", path], capture_output=True, text=True, encoding="utf-8")
    if r.returncode != 0:
        raise RuntimeError(r.stderr.strip() or r.stdout.strip())
    return json.loads(r.stdout)


def fetch_doc(repo, path):
    """(blob sha, text) of a file on the default branch (read-only `gh api repos/<repo>/contents/<path>`), or (None, None) on failure."""
    try:
        sha, text = decode_contents(gh_json("repos/%s/contents/%s" % (repo, path)))
        if text is None:
            r = subprocess.run(["gh", "api", "repos/%s/contents/%s" % (repo, path), "-H", "Accept: application/vnd.github.raw"],
                               capture_output=True, text=True, encoding="utf-8")
            text = r.stdout if r.returncode == 0 else None
        return sha, text
    except (SystemExit, Exception) as e:
        print("pr-scan: fetching %s failed: %s" % (path, e), file=sys.stderr)
        return None, None


def fetch_commits(repo, now, days=CLOSED_DAYS):
    """Commits on the default branch of the last `days` days (read-only `gh api repos/<repo>/commits?since=`, 100 per page, up to
    COMMIT_PAGES pages), or None when the query fails."""
    since = (now - dt.timedelta(days=days)).strftime(TS_FMT)
    out = []
    try:
        for page in range(1, COMMIT_PAGES + 1):
            got = gh_json("repos/%s/commits?since=%s&per_page=100&page=%d" % (repo, since, page))
            out += parse_commits(got)
            if not isinstance(got, list) or len(got) < 100:
                break
        return out
    except (SystemExit, Exception) as e:
        print("pr-scan: commit query failed: %s" % e, file=sys.stderr)
        return None


def build_progress(repo, now):
    """Everything the Progress page shows besides the PR analysis: the assessment file, the live blob shas, the parsed README and HANDOFF
    and the recent commits. A part that cannot be fetched or parsed is None / empty (the page says "No data yet")."""
    r_sha, readme = fetch_doc(repo, "README.md")
    h_sha, handoff = fetch_doc(repo, "HANDOFF.md")
    return {"assessment": load_progress(), "live": {"README": r_sha, "HANDOFF": h_sha},
            "features": parse_features(readme) if readme else None, "games": parse_games(readme) if readme else [],
            "highlights": parse_highlights(readme) if readme else [], "next": parse_next_steps(handoff) if handoff else [],
            "inprogress": parse_in_progress(handoff) if handoff else None, "commits": fetch_commits(repo, now)}


# -- HTML

def banner_html(assess, live, repo):
    docs = changed_docs(assess, live)
    if not docs:
        return ""
    links = " ".join('<a href="https://github.com/%s/commits/main/%s.md">%s history</a>' % (esc(repo), n, n) for n in docs)
    return '<p class="pg-banner" role="status">%s changed since this assessment (%s). %s</p>' % (" and ".join(docs), esc(assess["assessedAt"]), links)


def _norm_name(s):
    return re.sub(r"[^a-z0-9]+", "", s.lower())


def ladder_bar_html(assess, ov, markers):
    steps = _ladder_sorted(assess["ladder"])
    ticks = "".join('<li style="left:%s%%"%s><b>%s</b>%s</li>' % (fmt_pct(s["pct"]), ' class="end"' if s["pct"] >= 100 else "", fmt_pct(s["pct"]), esc(s["label"]))
                    for s in steps)
    rows = max([m["row"] for m in markers] + [0]) + 1
    mk = "".join('<i class="pg-mk %s" style="left:%s%%;bottom:%dpx" role="img" aria-label="%s" data-tip="%s"></i>' % (
        "ok" if m["playable"] else "wip", fmt_pct(m["pct"]), m["row"] * 11, esc("%s, %s %%" % (m["name"], fmt_pct(m["pct"]))),
        esc("%s: %s %%" % (m["name"], fmt_pct(m["pct"])))) for m in markers)
    return ('<div class="pg-bar" style="--rows:%d" role="img" aria-label="Overall %s %%, %s"><div class="pg-marks">%s</div>'
            '<div class="pg-track"><i style="width:%s%%"></i></div><ol class="pg-ticks">%s</ol></div>' % (
                rows, fmt_pct(ov["pct"]), esc(ov["step"]), mk, fmt_pct(ov["pct"]), ticks))


def game_cells_html(game, ladder):
    steps = _ladder_sorted(ladder)
    cells = []
    for s in steps:
        lv = 1.0 if s["key"] == "groundwork" else _clamp01(game["levels"].get(s["key"], 0))
        kind = "full" if lv >= 1 else "part" if lv > 0 else "none"
        cells.append('<i class="c %s" style="--f:%d%%" data-tip="%s"></i>' % (kind, round(lv * 100), esc("%s (%s %%): %s" % (
            s["label"], fmt_pct(s["pct"]), "reached" if kind == "full" else "%d %% of the way" % round(lv * 100) if kind == "part" else "not reached"))))
    return '<div class="pg-cells">%s</div>' % "".join(cells)


def progress_games_html(pg, repo):
    a = pg["assessment"]
    readme = {_norm_name(g["name"]): g for g in pg["games"]}
    o, known = [], set()
    if a:
        o.append('<div class="pg-cols" aria-hidden="true">%s</div>' % "".join("<span>%s</span>" % esc(s["label"]) for s in _ladder_sorted(a["ladder"])))
        o.append("<ul class=\"pg-games\">")
        for g in sorted(a["games"], key=lambda g: (is_playable(g), -game_pct(g["levels"], a["ladder"]))):  # games still in progress first
            r = readme.get(_norm_name(g["name"]))
            known.add(_norm_name(g["name"]))
            pill = ""
            if r:
                pill = '<span class="pg-st">%s</span>' % esc(r["state"])
            o.append('<li class="pg-game"><div class="pg-gh"><b>%s</b><span class="pg-gp">%s %%</span>%s</div>%s'
                     '<p class="small pg-ev">%s<a href="%s#games" aria-label="README, Games table">README</a></p>%s</li>' % (
                         esc(g["name"]), fmt_pct(game_pct(g["levels"], a["ladder"])), pill, game_cells_html(g, a["ladder"]),
                         ("“%s” " % esc(g["evidence"])) if g["evidence"] else "",
                         "https://github.com/%s/blob/main/README.md" % esc(repo),
                         '<p class="small pg-lim"><b>Limit</b> %s</p>' % esc(g["limits"]) if g["limits"] else ""))
        for g in pg["games"]:
            if _norm_name(g["name"]) not in known:
                o.append('<li class="pg-game"><div class="pg-gh"><b>%s</b><span class="pg-gp">not assessed yet</span></div><p class="small pg-ev">%s</p></li>' % (
                    esc(g["name"]), esc(g["what"])))
        o.append("</ul>")
    elif pg["games"]:
        o.append('<p class="empty-block">%s</p><ul class="pg-games">%s</ul>' % (NO_DATA, "".join(
            '<li class="pg-game"><div class="pg-gh"><b>%s</b>%s</div><p class="small pg-ev">%s</p></li>' % (
                esc(g["name"]), '<span class="pill">%s</span>' % esc(g["state"]), esc(g["what"])) for g in pg["games"])))
    else:
        o.append('<p class="empty-block">%s</p>' % NO_DATA)
    if pg["highlights"]:
        o.append('<h3 class="pg-sub">Highlights</h3><ul class="pg-hl">%s</ul>' % "".join("<li>%s</li>" % inline_html(h) for h in pg["highlights"]))
    return "".join(o)


def progress_features_html(feat):
    if not feat:
        return '<p class="empty-block">%s</p>' % NO_DATA
    o = ['<div class="pg-sec all"><span class="nm">All sections</span><span class="pg-meter"><i style="width:%d%%"></i></span><span class="pc">%d %%</span>'
         '<span class="small">%d of %d</span></div>' % (feat["pct"], feat["pct"], feat["done"], feat["total"])]
    for s in feat["sections"]:
        rows = "".join('<li><span class="pg-st">%s</span><b>%s</b><span class="small" data-tip="%s">%s</span></li>' % (
            esc(r["state"]), esc(r["feature"]), esc(r["what"]),
            esc(r["what"] if len(r["what"]) <= 110 else r["what"][:109].rstrip() + "…")) for r in s["rows"])
        o.append('<details class="pg-sec"><summary><span class="nm">%s</span><span class="pg-meter"><i style="width:%d%%"></i></span><span class="pc">%d %%</span>'
                 '<span class="small">%d of %d</span></summary><ul class="pg-rows">%s</ul></details>' % (
                     esc(s["name"]), s["pct"], s["pct"], s["done"], s["total"], rows))
    return "".join(o)


def progress_next_html(pg, prs):
    if not pg["next"]:
        return '<p class="empty-block">%s</p>' % NO_DATA
    o = ['<ol class="pg-next">']
    for it in pg["next"]:
        rel = related_prs(it, prs)
        chips = "".join('<a class="fchip" href="%s" data-tip="%s">#%d</a>' % (esc(safe_url(r["url"])), esc(r["title"]), r["number"]) for r in rel)
        full = plain_md(it["text"])
        o.append('<li value="%d"><details><summary>%s</summary>%s</details><div class="pg-prs">%s</div></li>' % (
            it["n"], inline_html(it["short"]), "<p class=\"small\">%s</p>" % inline_html(full) if full != it["short"] else "",
            chips if chips else '<span class="small">No related open PR</span>'))
    o.append("</ol>")
    ip = pg["inprogress"]
    o.append('<h3 class="pg-sub">In progress (HANDOFF)</h3>')
    if ip is None:
        o.append('<p class="small">%s</p>' % NO_DATA)
    elif not ip:
        o.append('<p class="small">Nothing listed.</p>')
    else:
        o.append('<ul class="pg-hl">%s</ul>' % "".join("<li><b>%s</b> %s</li>" % (esc(x["heading"]), inline_html(x["line"])) if x["heading"]
                                                      else "<li>%s</li>" % inline_html(x["line"]) for x in ip))
    return "".join(o)


def progress_work_html(commits, view, now, range_s, rl, repo):
    """The body of "Being worked on now" for one range: upstream commits (up to COMMITS_SHOWN) and the most active areas."""
    o = []
    if commits is None:
        o.append('<p class="empty-block">%s</p>' % NO_DATA)
    else:
        cs = commits_in_range(commits, range_s, now)
        full = len(commits) >= COMMIT_PAGES * 100
        o.append('<p class="pg-count"><b>%s%d</b> commit%s on main in the last %s</p>' % (
            "" if len(cs) < len(commits) or not full else "≥", len(cs), "" if len(cs) == 1 else "s", esc(rl)))
        if cs:
            o.append('<ul class="pg-commits">%s</ul>' % "".join(
                '<li><a href="%s">%s</a> %s <span class="small">%s &middot; %s</span></li>' % (
                    esc(safe_url(c["url"])), esc(c["sha"]), esc(c["subject"]), esc(c["author"]), esc(c["ts"][:10])) for c in cs[:COMMITS_SHOWN]))
        else:
            o.append('<p class="small">No commits in the last %s.</p>' % esc(rl))
    areas = active_areas(view)
    o.append('<h3 class="pg-sub">Most active areas</h3>')
    if areas:
        top = max(n for _, n in areas)
        o.append('<ul class="pg-areas">%s</ul>' % "".join(
            '<li>%s<span class="pg-meter"><i style="width:%d%%"></i></span><b>%d</b></li>' % (ptype(a), round(100 * n / top), n) for a, n in areas))
    else:
        o.append('<p class="small">No PR activity in the last %s.</p>' % esc(rl))
    return "".join(o)


def progress_hero_html(pg, repo):
    a = pg["assessment"]
    if not a:
        return '<section class="pg-hero a-prog" aria-label="Overall progress"><p class="empty-block">%s</p></section>' % NO_DATA
    ov = overall_progress(a["games"], a["ladder"])
    done = "all %d games" % ov["total"] if ov["playable"] == ov["total"] else "%d of %d games" % (ov["playable"], ov["total"])
    note = ('<p class="score-note"><b>%s</b><span>%s playable</span><span>Assessed %s (%s)</span></p>' % (
        esc(ov["step"]) if ov["game"] else "Everything playable", done, esc(a["assessedAt"]), esc(a["assessedBy"])))
    return ('<section class="pg-hero a-prog" aria-label="Overall progress"><div class="pg-score"><div class="score">%s<small>%%</small></div>%s</div>%s%s</section>' % (
        fmt_pct(ov["pct"]), note, ladder_bar_html(a, ov, ladder_markers(a["games"], a["ladder"])), banner_html(a, pg["live"], repo)))


PROGRESS_CSS = """
.pg{display:flex;flex-direction:column;gap:var(--sp-2);min-width:0;min-height:0}
.pg-hero,.pg-panel{background:var(--surface);border:1px solid var(--edge);min-width:0;min-height:0}
.pg-hero{display:grid;grid-template-columns:minmax(0,1fr);gap:var(--sp-3) var(--sp-5);align-items:end;padding:var(--sp-3) var(--pan-x)}
.pg-hero>.empty-block{grid-column:1/-1}
.pg-score{display:flex;align-items:flex-end;gap:var(--sp-2) var(--sp-4);flex-wrap:wrap}.pg-score .score-box,.pg-score .score-note{margin:0}
.pg-banner{grid-column:1/-1;padding:var(--sp-2) var(--sp-3);border-left:4px solid var(--partial);background:var(--wash);color:var(--ink);font-size:13px;line-height:1.35}
.pg-bar{position:relative;min-width:0;padding-bottom:calc(var(--sp-5) + var(--sp-1))}
.pg-marks{position:relative;height:calc(var(--rows)*11px + 6px);margin-bottom:var(--sp-1)}
.pg-mk{position:absolute;display:block;width:9px;height:9px;margin-left:-5px;border:1px solid var(--page)}
.pg-mk.ok{background:var(--good)}.pg-mk.wip{background:var(--partial)}
.pg-track{position:relative;height:14px;background:var(--wash)}.pg-track i{display:block;height:100%;background:var(--accent)}
.pg-ticks{position:absolute;left:0;right:0;top:calc(var(--rows)*11px + 6px + var(--sp-1) + 14px)}
.pg-ticks li{position:absolute;top:0;transform:translateX(-50%);padding-top:calc(var(--sp-2) + 2px);text-align:center;font-size:12px;line-height:1.2;color:var(--soft);white-space:nowrap}
.pg-ticks li:before{content:"";position:absolute;left:50%;top:0;height:var(--sp-2);border-left:1px solid var(--ink)}
.pg-ticks li b{display:block;font:400 13px/1.2 Anton,Impact,sans-serif;color:var(--ink)}
.pg-ticks li.end{transform:translateX(-100%);text-align:right}.pg-ticks li.end:before{left:auto;right:0}
.pg-grid{display:grid;grid-template-columns:minmax(0,1fr);gap:var(--sp-2);min-height:0}
.pg-panel{display:flex;flex-direction:column;padding:var(--sp-3) var(--pan-x)}
.pg-body{overflow:auto;min-height:0;max-height:380px;flex:1;scrollbar-gutter:stable;scrollbar-width:thin;scrollbar-color:var(--rule) transparent}
.pg-sub{font:400 15px/1.2 Anton,Impact,sans-serif;letter-spacing:.03em;margin:var(--sp-3) 0 var(--sp-1)}
.pg-cols{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));gap:var(--sp-1);position:sticky;top:0;z-index:1;background:var(--surface);padding-bottom:var(--sp-1);font-size:11px;line-height:1.2;color:var(--soft);text-align:center}
.pg-cols span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.pg-game{padding:var(--row-y) 0;border-top:1px solid var(--rule)}
.pg-gh{display:flex;align-items:baseline;gap:var(--sp-2);min-width:0}.pg-gh b{font-size:14px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0}
.pg-gp{margin-left:auto;font:400 16px/1 Anton,Impact,sans-serif;white-space:nowrap}.pg-st{font-size:13px;line-height:1.3;white-space:nowrap}
.pg-cells{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));gap:var(--sp-1);margin:var(--sp-1) 0}
.pg-cells .c{display:block;height:8px;background:var(--empty)}.pg-cells .c.full{background:var(--good)}.pg-cells .c.part{background:linear-gradient(90deg,var(--partial) var(--f),var(--empty) var(--f))}
.pg-ev,.pg-lim{margin:0;overflow-wrap:anywhere}.pg-ev a{margin-left:var(--sp-1)}.pg-lim b{color:var(--partial-text);font-weight:500}
.pg-hl{padding-left:var(--sp-4);list-style:disc;font-size:13px;line-height:1.4}.pg-hl li+li{margin-top:var(--sp-1)}
.pg-sec{display:grid;grid-template-columns:minmax(0,1fr) minmax(40px,1fr) 40px auto;gap:var(--sp-2);align-items:center;font-size:13px}
.pg-sec.all{padding:var(--sp-1) 0 var(--sp-2);font-weight:500}
details.pg-sec{display:block;border-top:1px solid var(--rule)}
details.pg-sec>summary{display:grid;grid-template-columns:minmax(0,1fr) minmax(40px,1fr) 40px auto;gap:var(--sp-2);align-items:center;cursor:pointer;padding:var(--sp-2) 0;list-style:none}
details.pg-sec>summary::-webkit-details-marker{display:none}
.pg-sec .nm{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500}.pg-sec .pc{text-align:right;font-family:Anton,Impact,sans-serif}
.pg-meter{display:block;height:8px;background:var(--wash)}.pg-meter i{display:block;height:100%;background:var(--good)}
.pg-rows li{display:grid;grid-template-columns:auto minmax(0,1fr);gap:0 var(--sp-2);align-items:baseline;padding:var(--sp-1) 0 var(--sp-1) var(--sp-3);font-size:13px}
.pg-rows li .small{grid-column:2}.pg-rows li b{font-weight:500;overflow-wrap:anywhere}
.pg-next{display:grid;gap:var(--sp-1)}.pg-next>li{padding:var(--row-y) 0;border-top:1px solid var(--rule);font-size:13px;line-height:1.4}
.pg-next summary{cursor:pointer;font-weight:500;overflow-wrap:anywhere}.pg-next details p{margin-top:var(--sp-1);overflow-wrap:anywhere}
.pg-prs{display:flex;flex-wrap:wrap;gap:var(--pill-gap);margin-top:var(--sp-1)}
.pg-panel code{font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--wash);padding:0 var(--sp-1);overflow-wrap:anywhere}
.pg-count{font-size:13px;color:var(--soft);margin-bottom:var(--sp-1)}.pg-count b{font:400 20px/1 Anton,Impact,sans-serif;color:var(--ink);margin-right:var(--sp-1)}
.pg-commits li{display:block;padding:var(--sp-1) 0;border-top:1px solid var(--rule);font-size:13px;line-height:1.35;overflow-wrap:anywhere}
.pg-commits li a{font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;margin-right:var(--sp-1)}
.pg-areas li{display:grid;grid-template-columns:minmax(0,1.4fr) minmax(24px,1fr) 24px;gap:var(--sp-2);align-items:center;padding:var(--sp-1) 0;font-size:13px}
.pg-areas li b{text-align:right}
@media (min-width:900px){
.pg-hero{grid-template-columns:auto minmax(0,1fr)}
.js .pg{flex:1;min-height:0}
.js .pg-grid{flex:1;grid-template-columns:repeat(2,minmax(0,1fr));grid-template-rows:minmax(0,1.45fr) minmax(0,1fr);grid-template-areas:"games next" "check work"}
.pg-games-p{grid-area:games}.pg-check-p{grid-area:check}.pg-next-p{grid-area:next}.pg-work-p{grid-area:work}
.js .pg-body{max-height:none}
.pg-panel{padding:var(--sp-3) var(--pan-x)}
.pg-hero .score{font-size:clamp(34px,7.4vh,72px)}
}
"""


PAGES = [("progress", "Progress"), ("effort", "Effort"), ("areas", "Areas"), ("trends", "Trends"), ("stats", "Stats")]
DEFAULT_PAGE = PAGES[0][0]
CHART_WIDE, CHART_HALF = (640, 240), (320, 240)   # viewBox of the large trend charts: a wide one and two half-width ones


def render_html(an, repo, now, hist=None, progress=None):
    hist = hist or []
    pg = progress or {"assessment": None, "live": {}, "features": None, "games": [], "highlights": [], "next": [], "inprogress": None, "commits": None}
    prs = an["prs"]
    closed = an.get("closed")
    has_closed = closed is not None
    by_num = {p["number"]: p for p in prs + (closed or [])}
    hot_set = set(an["clusterHot"]) if "clusterHot" in an else {h["file"] for h in an["hotFiles"]}
    areas = sorted(set(an["areas"]) | {p["primary"] for p in closed or []}, key=lambda a: (-len(an["areas"].get(a, [])), a))
    top = top_areas(an)
    n_open, n_un, n_dr = len(prs), sum(p["unreviewed"] for p in prs), sum(bool(p["isDraft"]) for p in prs)
    n_done, n_stale = sum(p["looksDone"] for p in prs), sum(p["stale"] for p in prs)
    n_fail = sum(p["ci"] in ("FAILURE", "ERROR") for p in prs)
    n_clusters = len(an["clusters"])
    spark_hist = window(hist, now, SPARK_DAYS)

    # one dataset per time range, all computed here: scene, Areas rows, counts and tab texts (the JS only swaps them)
    scenes, rdata = {}, {}
    for key, secs, rl in RANGES:
        view = range_view(an, secs, now)
        scenes[key] = effort_scene(view, rl=rl)
        c = range_counts(an, secs, now)
        rdata[key] = {"s": secs, "rl": rl, "counts": c, "metas": range_metas(c, rl), "standings": standings_html(view, rl, spark_hist), "sel": select_counts(top, view),
                      "work": progress_work_html(pg["commits"], view, now, secs, rl, repo),
                      "line": "Last %s: %d active · %s merged · %s closed" % (
                          rl, c["active"], c["merged"] if has_closed else "?", c["closed"] if has_closed else "?") if has_closed
                      else "Last %s: %d active" % (rl, c["active"])}
    scene = scenes[DEFAULT_RANGE]
    any_tower = any(sc["towers"] for sc in scenes.values())
    rl0 = DEFAULT_RL

    def plink(n):
        return '<a href="%s">#%d</a>' % (esc(safe_url(by_num[n]["url"])), n)

    def dl(key, cur):
        return change_24h(hist, key, cur, now)

    def pr_line(p):
        return '<a href="%s">#%d</a> %s' % (esc(safe_url(p["url"])), p["number"], esc(p["title"]))

    o = []
    area_opts = ['<option value="">All areas</option>']
    for e in top:
        if not e["members"]:
            area_opts.append('<option value="%s" data-name="%s">%s (%d)</option>' % (esc(slug(e["name"])), esc(area_label(e["name"])), esc(area_label(e["name"])), len(e["prs"])))
            continue
        area_opts.append('<optgroup label="%s"><option value="%s" data-members="%s" data-name="All %s">All %s (%d)</option>%s</optgroup>' % (
            esc(e["name"]), esc(entry_slug(e)), esc(",".join(slug(m) for m in e["members"])), esc(e["name"]), esc(e["name"]), len(e["prs"]),
            "".join('<option value="%s" data-name="%s">%s (%d)</option>' % (esc(slug(m)), esc(area_label(m)), esc(area_label(m)), len(an["areas"].get(m, []))) for m in e["members"])))
    toolbar = ('<div class="toolbar" id="filters" role="search"><label class="vh" for="q">Search pull requests</label>'
               '<input id="q" type="search" placeholder="Search title, number, author or path" autocomplete="off">'
               '<label class="vh" for="area">Area</label><select id="area">%s</select>'
               '<div class="seg" id="range" role="group" aria-label="Time range">%s</div>'
               '<div class="seg" id="states" role="group" aria-label="State">%s</div><span class="result" id="result" role="status"></span></div>' % (
                   "".join(area_opts),
                   "".join('<button type="button" data-range="%s" aria-pressed="%s">%s</button>' % (k, "true" if k == DEFAULT_RANGE else "false", lb)
                           for k, _, lb in RANGES),
                   "".join('<button type="button" data-state="%s" aria-pressed="%s">%s</button>' % (k, "true" if not k else "false", n)
                           for k, n in (("", "All"), ("unrev", "Unreviewed"), ("done", "Ready"), ("draft", "Draft"), ("closed", "Closed")))))
    o.append("<!doctype html><html lang=en><head><meta charset=utf-8>"
             "<meta name=viewport content='width=device-width,initial-scale=1'>"
             "<title>PR scan: %s</title>"
             "<link rel=preconnect href='https://fonts.googleapis.com'><link rel=preconnect href='https://fonts.gstatic.com' crossorigin>"
             "<link rel=stylesheet href='%s'><script>%s</script><style>%s%s</style></head><body><div class=\"app\">" % (
                 esc(repo), FONTS_URL, THEME_JS, CSS, SCENE_CSS + PROGRESS_CSS))
    o.append('<header class="top"><div class="nav-row"><h1>PR scan <span>/ <a href="https://github.com/%s">%s</a></span></h1>'
             '<nav class="ptabs" id="ptabs" aria-label="Pages">%s</nav>'
             '<div class="top-right"><div class="live"><i class="dot" aria-hidden="true"></i>'
             '<span><span id="ago">Updated </span><time id="upd" datetime="%s">%s UTC</time></span></div>'
             '<button type="button" class="theme" id="theme" aria-label="Colour theme">Theme</button></div></div>%s</header>' % (
                 esc(repo), esc(repo),
                 "".join('<button type="button" data-page="%s" aria-pressed="%s">%s</button>' % (k, "true" if k == DEFAULT_PAGE else "false", n) for k, n in PAGES),
                 now.strftime(TS_FMT), now.strftime("%Y-%m-%d %H:%M"), toolbar))
    o.append('<main class="pages" id="pages">')

    def page_open(k):
        return '<div class="page" id="page-%s" data-page="%s"><h2 class="page-title">%s</h2>' % (k, k, esc(dict(PAGES)[k]))

    # ---- Progress page: overall hero and ladder, games, README checklist, what is next, what is being worked on
    o.append(page_open("progress"))
    o.append('<div class="pg" id="pg">%s<div class="pg-grid">' % progress_hero_html(pg, repo))
    nfeat = pg["features"]
    o.append('<section class="pg-panel pg-games-p" aria-label="Games"><div class="head"><h2>Games</h2><span>%s</span></div><div class="pg-body">%s</div></section>' % (
        "assessed by %s on %s" % (esc(pg["assessment"]["assessedBy"]), esc(pg["assessment"]["assessedAt"])) if pg["assessment"] else NO_DATA.lower(),
        progress_games_html(pg, repo)))
    o.append('<section class="pg-panel pg-check-p" aria-label="README feature checklist"><div class="head"><h2>What works</h2><span>%s</span></div><div class="pg-body">%s</div></section>' % (
        "%d of %d done" % (nfeat["done"], nfeat["total"]) if nfeat else NO_DATA.lower(), progress_features_html(nfeat)))
    o.append('<section class="pg-panel pg-next-p" aria-label="What is next"><div class="head"><h2>What&rsquo;s next</h2><span>HANDOFF, by priority</span></div><div class="pg-body">%s</div></section>' % (
        progress_next_html(pg, prs)))
    o.append('<section class="pg-panel pg-work-p" aria-label="Being worked on now"><div class="head"><h2>Being worked on now</h2><span>main commits and areas, last <span id="pg-rl">%s</span></span></div>'
             '<div class="pg-body" id="pg-work">%s</div></section>' % (esc(rl0), rdata[DEFAULT_RANGE]["work"]))
    o.append("</div></div></div>")

    # ---- Effort page: hero, effort map, road
    d_open = change_24h(hist, "open", n_open, now)
    o.append(page_open("effort"))
    o.append('<section class="a-city" aria-label="Effort map"><div class="scene%s"><div class="score-box"><div class="score">%d<small>all open</small></div>'
             '<p class="score-note"><b>%d unreviewed &middot; %d look done</b><span>%d possible duplicate group%s &middot; %d draft%s</span>%s'
             '<span id="range-line">%s</span></p></div>' % (
                 " has-stage" if any_tower else "", n_open, n_un, n_done, n_clusters, "" if n_clusters == 1 else "s",
                 n_dr, "" if n_dr == 1 else "s", delta_text(d_open), esc(rdata[DEFAULT_RANGE]["line"])))
    if any_tower:
        sranges = {k: scene_range(sc) for k, sc in scenes.items()}
        o.append('<div class="stage" id="stage"%s>%s%s</div><p class="empty-block" id="scene-empty"%s>%s</p>'
                 '<div class="scene-foot">%s</div>'
                 '<ol class="scene-key">%s</ol><script type="application/json" id="scene-data">%s</script>' % (
                     "" if scene["towers"] else " hidden", scene_svg(scene), FOCUS_PANEL, "" if not scene["towers"] else " hidden", esc(empty_range(rl0)),
                     legend([(n, "k-" + k) for k, n, _ in SCENE_STATES if has_closed or k != "merged"]),
                     scene_key_html(scene), scene_json(scene, sranges, scene_titles(by_num, sranges))))
    else:
        o.append('<p class="empty-block">No data yet</p><p class="sec-note">The effort map needs at least one open pull request.</p>')
    o.append("</div></section>")
    cnt_all = {k: sum(bucket(p) == k for p in prs) for k, _ in BUCKETS}
    merged24 = an.get("merged24")
    o.append('<section class="a-road" aria-label="Pull request lifecycle"><div class="head"><h2>Road to merged</h2><span>%d all open</span></div><ol class="road">' % n_open)
    for (k, name), color in zip(BUCKETS, ("var(--soft)", "var(--partial)", "var(--accent)", "var(--good)")):
        o.append(road_step({"wait": "Waiting", "done": "Ready"}.get(k, name), cnt_all[k], cnt_all[k] / n_open if n_open else 0, color))
    o.append(road_step("Merged 24 h", merged24, None, "var(--good)", ""))
    o.append("</ol></section></div>")

    # ---- Areas page: standings in full
    o.append(page_open("areas"))
    o.append('<section class="a-standings" aria-label="Areas"><div class="head"><h2>By area</h2><span>top %d by open PRs, line = %dd</span></div>'
             '<div style="margin-bottom:8px">%s</div><div class="st-list"><div class="st-axis" aria-hidden="true"><span>#</span><span>Area</span>'
             '<span>State mix</span><span>Open</span></div><ol id="standings">%s</ol></div></section></div>' % (
                 TOP_AREAS, SPARK_DAYS, legend([(n, "k-" + k) for k, n in BUCKETS] + ([(MERGED_LABEL, "k-merged")] if has_closed else [])),
                 rdata[DEFAULT_RANGE]["standings"]))

    # ---- Trends page: three large charts
    recent = window(hist, now, TREND_DAYS)
    o.append(page_open("trends"))
    o.append('<section class="a-trends" aria-label="Trends"><div class="head"><h2>Trends</h2><span>last %d days, %d snapshot%s</span></div>' % (
        TREND_DAYS, len(recent), "" if len(recent) == 1 else "s"))
    blocks = []
    if len(recent) < 2:
        o.append('<p class="empty-block">No data yet</p><p class="sec-note">Trends start after the first hours of hourly snapshots (%d so far).</p>' % len(recent))
    else:
        days = [x for x in daily_summary(recent) if x["pairs"]] if len({x["ts"][:10] for x in recent}) >= 2 else []
        blocks.append('<div class="chart-block wide"><div class="chart-head"><h3>Open and unreviewed</h3>%s</div><div class="chart-box">%s</div></div>' % (
            legend([("Open", "s-open"), ("Unreviewed", "s-unrev")]),
            svg_line_chart([
                {"name": "open", "cls": "s-open", "points": [(parse_time(x["ts"]), x["open"]) for x in recent]},
                {"name": "unreviewed", "cls": "s-unrev", "points": [(parse_time(x["ts"]), x["unreviewed"]) for x in recent]}],
                "Open and unreviewed pull requests over the last %d days" % TREND_DAYS, *CHART_WIDE)))
        blocks.append('<div class="chart-block"><div class="chart-head"><h3>Opened and closed per day</h3>%s</div><div class="chart-box">%s</div></div>' % (
            legend([("Opened", "b-new"), ("Closed or merged", "b-closed")]),
            svg_bars(days, "Pull requests opened and closed per day (UTC)", *CHART_HALF)))
    mpd = merged_per_day(closed, now)
    blocks.append('<div class="chart-block"><div class="chart-head"><h3>Merged per day</h3>%s</div><div class="chart-box">%s</div></div>' % (
        legend([("Merged", "b-merged")]),
        svg_bars(mpd, "Pull requests merged per day (UTC), from the closed PRs of the last %d days" % CLOSED_DAYS, *CHART_HALF,
                 series=(("merged", "b-merged", "merged"),))))
    o.append('<div class="charts">%s</div></section></div>' % "".join(blocks))

    # ---- Stats page
    o.append(page_open("stats"))
    c0 = rdata[DEFAULT_RANGE]["counts"]
    o.append('<section class="a-meta" aria-label="Overall stats">%s</section></div>' % "".join((
        stat("Open", n_open, dl("open", n_open), cls="key"),
        stat("Active (%s)" % rl0, c0["active"], show_delta=False, rstat=("active", "Active")),
        stat("Merged (%s)" % rl0, c0["merged"] if has_closed else "?", show_delta=False, rstat=("merged", "Merged")),
        stat("Closed (%s)" % rl0, c0["closed"] if has_closed else "?", show_delta=False, rstat=("closed", "Closed")),
        stat("Unreviewed", n_un, dl("unreviewed", n_un)),
        stat("Looks done", n_done, dl("looksDone", n_done)),
        stat("Possible duplicates", n_clusters, dl("clusters", n_clusters)),
        stat("Drafts", n_dr, dl("draft", n_dr)),
        stat("Stale (%dd+)" % STALE_DAYS, n_stale, show_delta=False),
        stat("CI failing", n_fail, show_delta=False, cls="warn" if n_fail else ""))))
    o.append("</main>")

    # ---- feed: tabs, on every page
    done = sorted([p for p in prs if p["looksDone"]], key=lambda p: (-p["ageDays"], p["number"]))
    metas = {"dup": "%d group%s" % (n_clusters, "" if n_clusters == 1 else "s"), "ready": "%d, oldest first" % len(done),
             "hot": "%d file%s" % (len(an["hotFiles"]), "" if len(an["hotFiles"]) == 1 else "s"), "all": "%d pull requests" % n_open,
             "closed": ("%d merged, %d closed, last %d d" % (len(an["mergedPrs"]), len(closed) - len(an["mergedPrs"]), CLOSED_DAYS)
                        if has_closed else "no data yet")}
    titles = [("dup", "Duplicates"), ("ready", "Ready to merge"), ("hot", "Hot files"), ("all", "All PRs"), ("closed", "Closed")]
    o.append('<section class="a-feed" aria-label="Pull request lists"><div class="head"><div class="tabs">%s</div><span id="tabmeta"></span></div><div class="feed-body" id="feed">' % "".join(
        '<button type="button" data-tab="%s" aria-pressed="%s">%s</button>' % (k, "true" if k == "dup" else "false", n) for k, n in titles))

    def panel(key, body, note=""):
        name = dict(titles)[key]
        return ('<div class="panel" data-panel="%s" data-meta="%s"><div class="ptitle"><h2>%s</h2><span>%s</span></div>%s%s'
                '<p class="empty-block range-empty" hidden>%s</p></div>' % (
                    key, esc(metas[key]), esc(name), esc(metas[key]), '<p class="sec-note">%s</p>' % note if note else "", body,
                    esc(empty_range(rl0))))

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
            '<div class="row rg" %s><div class="lead">%s</div><div><div class="ttl">%s</div>'
            '<div class="sys"><span>by %s</span>%s</div></div></div>' % (
                ts_attrs(by_num[n]), avatar(by_num[n]["author"]), pr_line(by_num[n]), esc(by_num[n]["author"]),
                closed_tag(by_num[n]) if by_num[n].get("state", "OPEN") != "OPEN" else state_pills(by_num[n]))
            for n in c["members"])
        sup = ('<p class="small sup">Possibly superseded: %s</p>' % "".join(
            '<span class="sm rg" %s>%s merged</span>' % (ts_attrs(by_num[n]), plink(n)) for n in c["merged"])) if c["superseded"] else ""
        label, label_area = cluster_label([by_num[n] for n in c["members"]])
        dup.append('<div class="dup" style="%s"><div class="chead"><h3>%s</h3><span class="small">%s</span></div>%s%s%s'
                   '<p class="small">Why: %s</p></div>' % (
                       cat_style(label_area), esc(label), esc(", ".join("#%d" % n for n in c["members"])), sup, members,
                       '<div class="chips-f">%s</div>' % chips if chips else "", esc(cluster_why(c))))
    ready = ['<p class="empty-block">None</p>'] if not done else [
        '<div class="row rd rg" %s><div class="lead"><b>%dd</b>old</div><div><div class="ttl">%s</div><div class="sys">%s%s'
        '<span><span class="add">+%d</span> <span class="del">&minus;%d</span> &middot; %s files</span></div></div></div>' % (
            ts_attrs(p), p["ageDays"], pr_line(p), ptype(p["primary"]), ci_pill(p["ci"]), p["additions"], p["deletions"], esc(pr_files_note(p)))
        for p in done]
    hot = ['<p class="empty-block">None</p>'] if not an["hotFiles"] else [
        '<div class="row ht" data-n="%d"><div class="lead"><b>%d</b>PRs</div><div><div class="mono">%s</div>'
        '<span class="heat"><i style="width:%d%%"></i></span><div class="sys">%s</div></div></div>' % (
            h["count"], h["count"], esc(h["file"]), round(100 * h["count"] / max(x["count"] for x in an["hotFiles"])),
            " ".join('<a class="rg" %s href="%s">#%d</a>' % (ts_attrs(by_num[n]), esc(safe_url(by_num[n]["url"])), n) for n in h["prs"]))
        for h in an["hotFiles"]]
    allp = []
    closed_by_area = {}
    for p in closed or []:
        closed_by_area.setdefault(p["primary"], []).append(p)
    for a in areas:
        ps = sorted(an["areas"].get(a, []), key=lambda p: p["number"])
        rows = []
        for p in ps:
            also = [x for x in p["areas"] if x != p["primary"]]
            q = " ".join([str(p["number"]), "#%d" % p["number"], p["title"], p["author"]] + [f["path"] for f in p.get("files", [])]).lower()
            rows.append(
                '<div class="row pr" data-area="%s" data-cl="0" %s data-draft="%d" data-unrev="%d" data-done="%d" data-q="%s"><div class="lead">%s</div>'
                '<div><div class="ttl">%s%s</div><div class="sys">%s<span>%dd</span>'
                '<span><span class="add">+%d</span> <span class="del">&minus;%d</span></span><span>%s files</span></div></div></div>' % (
                    esc(slug(a)), ts_attrs(p), bool(p["isDraft"]), p["unreviewed"], p["looksDone"], esc(q), avatar(p["author"]),
                    pr_line(p), ' <span class="small">(also %s)</span>' % esc(", ".join(area_label(x) for x in also)) if also else "",
                    state_pills(p), p["ageDays"], p["additions"], p["deletions"], esc(pr_files_note(p))))
        for p in closed_by_area.get(a, []):
            q = " ".join([str(p["number"]), "#%d" % p["number"], p["title"], p["author"]] + [f["path"] for f in p.get("files", [])]).lower()
            rows.append(
                '<div class="row pr" hidden data-area="%s" data-cl="1" %s data-draft="0" data-unrev="0" data-done="0" data-q="%s">'
                '<div class="lead"><b>%dd</b>ago</div><div><div class="ttl">%s</div><div class="sys">%s%s<span>by %s</span></div></div></div>' % (
                    esc(slug(a)), ts_attrs(p), esc(q), p["closedAgeDays"], pr_line(p), closed_tag(p), ptype(p["primary"]), esc(p["author"])))
        allp.append('<details class="area" id="%s"%s><summary>%s <span class="cnt">%d</span></summary>%s</details>' % (
            esc(slug(a)), "" if ps else " hidden", ptype(a), len(ps), "".join(rows)))
    if not has_closed:
        closed_rows = ['<p class="empty-block">No data yet</p>']
    elif not closed:
        closed_rows = ['<p class="empty-block">None in the last %d d</p>' % CLOSED_DAYS]
    else:
        closed_rows = [
            '<div class="row cl rg" %s><div class="lead"><b>%dd</b>ago</div><div><div class="ttl">%s</div><div class="sys">%s%s<span>by %s</span></div></div></div>' % (
                ts_attrs(p), p["closedAgeDays"], pr_line(p), closed_tag(p), ptype(p["primary"]), esc(p["author"]))
            for p in closed]
    o.append(panel("dup", "".join(dup), "Linked by changed files (Jaccard &ge; %.1f) or title words (Jaccard &ge; %.1f). Hot files are ignored." % (FILE_JACCARD, TITLE_JACCARD)))
    o.append(panel("ready", "".join(ready), "Not draft, CI green, and unreviewed or approved."))
    o.append(panel("hot", "".join(hot), "Touched by more than %g open PRs: conflict hotspots, left out of the duplicate groups." % an["hotThreshold"]))
    o.append(panel("all", "".join(allp)))
    o.append(panel("closed", "".join(closed_rows), "Pull requests closed in the selected range (up to %d days back), newest first. Merged ones form the violet base of the effort map." % CLOSED_DAYS))
    o.append("</div></section>")

    o.append('<footer class="foot">Generated %s UTC for %s. Read-only scan of public PR metadata.</footer></div>'
             '<div class="tip" id="tip" hidden></div><script type="application/json" id="range-data">%s</script><script>%s</script><script>%s</script></body></html>' % (
                 now.strftime("%Y-%m-%d %H:%M"), esc(repo),
                 json.dumps({"now": int(now.timestamp()), "default": DEFAULT_RANGE, "ranges": rdata}, separators=(",", ":")).replace("<", "\\u003c"),
                 JS, SCENE_JS))
    return "".join(o)


def render_md(an, repo, now, hist=None):
    hist = hist or []
    prs = an["prs"]
    by_num = {p["number"]: p for p in prs + (an.get("closed") or [])}

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
            md_esc(area_label(a)), slug(a), len(ps), c[0], c[1], c[2], c[3], sum(p["additions"] for p in ps), sum(p["deletions"] for p in ps)))
    o += ["", "## Possible duplicate work", ""]
    if not an["clusters"]:
        o += ["None.", ""]
    for c in an["clusters"]:
        o += ["- " + ", ".join(plink(by_num[n]) for n in c["members"]) + " : " + md_esc(cluster_why(c))]
        o += ["  - %s %s (%s)%s" % (plink(by_num[n]), md_esc(by_num[n]["title"]), md_esc(by_num[n]["primary"]),
                              " [merged]" if by_num[n].get("merged") else " [closed]" if by_num[n].get("state", "OPEN") != "OPEN" else "")
              for n in c["members"]]
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
        o += ['<a id="%s"></a>' % slug(a), "", "### %s (%d)" % (md_esc(area_label(a)), len(ps)), ""] + head + [prow(p) for p in ps] + [""]
    stale = [p for p in prs if p["stale"]]
    o += ["## Stale", "", "No update in %d days or more." % STALE_DAYS, ""]
    o += (head + [prow(p) for p in sorted(stale, key=lambda p: p["number"])]) if stale else ["None."]
    o += ["", "Generated %s UTC for %s." % (now.strftime("%Y-%m-%d %H:%M"), repo), ""]
    return "\n".join(o)


# ---------------------------------------------------------------- I/O (queries only)

PR_FIELDS = """
number title url body isDraft createdAt updatedAt state mergedAt closedAt reviewDecision
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
        "createdAt": n["createdAt"], "updatedAt": n["updatedAt"], "mergedAt": n.get("mergedAt"), "closedAt": n.get("closedAt"),
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


def fetch_search(repo, qualifiers):
    """Every PR of the repo matching the search qualifiers (paged search API), normalised."""
    split_repo(repo)
    return fetch_paged(SEARCH_QUERY, {"q": "repo:%s is:pr %s" % (repo, qualifiers)}, lambda d: d["search"])


def since_date(now, days):
    return (now - dt.timedelta(days=days)).strftime("%Y-%m-%d")


def fetch_merged(repo, now):
    return fetch_search(repo, "is:merged merged:>=%s" % since_date(now, MERGED_DAYS))


def fetch_closed(repo, now):
    """PRs closed (merged or not) in the last CLOSED_DAYS days, or None when the query fails (shown as "No data yet")."""
    try:
        return closed_window(fetch_search(repo, "is:closed closed:>=%s" % since_date(now, CLOSED_DAYS)), now)
    except (SystemExit, Exception) as e:
        print("pr-scan: closed-PR query failed: %s" % e, file=sys.stderr)
        return None


def write(path, text):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def public_pr(p):
    return {k: v for k, v in p.items() if k != "body"}


def merged_in_last(prs, now, hours):
    since = now - dt.timedelta(hours=hours)
    return sum(1 for p in prs if p.get("mergedAt") and parse_time(p["mergedAt"]) >= since)


def merged_last_24h(repo, now):
    """Number of PRs merged in the last 24 h, or None when the query fails (shown as "no data yet")."""
    try:
        return merged_in_last(fetch_merged(repo, now), now, 24)
    except (SystemExit, Exception) as e:
        print("pr-scan: merged-PR query failed: %s" % e, file=sys.stderr)
        return None


def cmd_report(args):
    now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    closed = fetch_closed(args.repo, now)
    an = build_analysis(fetch_open(args.repo), now, closed)
    an["merged24"] = merged_last_24h(args.repo, now) if closed is None else merged_in_last(closed, now, 24)
    os.makedirs(args.out, exist_ok=True)
    hist = []
    if args.history:
        hist = prune_history(append_snapshot(read_history(args.history), snapshot(an, now)), now)
        try:
            write(args.history, history_text(hist))
        except OSError as e:
            print("pr-scan: could not write history: %s" % e, file=sys.stderr)
    write(os.path.join(args.out, "index.html"), render_html(an, args.repo, now, hist, build_progress(args.repo, now)))
    write(os.path.join(args.out, "report.md"), render_md(an, args.repo, now, hist))
    data = {
        "repo": args.repo, "generatedAt": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "prs": [public_pr(p) for p in an["prs"]], "clusters": an["clusters"], "areaClusters": an["areaClusters"],
        "hotFiles": an["hotFiles"], "hotThreshold": an["hotThreshold"],
        "closed": None if an["closed"] is None else [public_pr(p) for p in an["closed"]],
    }
    write(os.path.join(args.out, "prs.json"), json.dumps(data, indent=2, ensure_ascii=False) + "\n")
    summary = {
        "repo": args.repo, "open": len(an["prs"]), "out": args.out, "clusters": len(an["clusters"]),
        "areas": {a: len(ps) for a, ps in sorted(an["areas"].items())},
        "merged": None if an["closed"] is None else len(an["mergedPrs"]),
        "closedUnmerged": None if an["closed"] is None else len(an["closed"]) - len(an["mergedPrs"]),
    }
    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        print("%s: %d open PRs, %d overlap clusters, written to %s" % (args.repo, summary["open"], summary["clusters"], args.out))
        if summary["merged"] is not None:
            print("last %d days: %d merged, %d closed without a merge" % (CLOSED_DAYS, summary["merged"], summary["closedUnmerged"]))
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
