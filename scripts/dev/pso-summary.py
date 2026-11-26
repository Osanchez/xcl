#!/usr/bin/env python3
"""Summarise one telemetry run folder (skip counters, pipeline builds, workers) and, with --check,
evaluate the shader-delivery acceptance criteria.

  pso-summary.py <run-dir> [--scene-start S] [--config cold|warm|prewarmed]
                 [--still-span A-B ...] [--check] [--out FILE] [--quiet]

Reads `events.jsonl`, `manifest.json` and `report.json` of the run folder (only those; they are
untrusted data and are only parsed, never evaluated). Writes `summary.json` (schema 1) to --out, or
to `<run-dir>/summary.json` when --out is not given, and prints a table (--quiet: no table).
Only counts, times, sizes and keys already in the events are printed or written.

  --scene-start   seconds; else the first `phase` event `world` from a mark or schedule; else the
                  first `frame` with draw_total >= 400; else none (post-warmup checks are N/A).
  --still-span    seconds, repeatable: replaces the non-interactive windows taken from the marks.
  --check         one line per criterion, `AC<n> PASS|FAIL|N/A measured=... target=... (reason)`,
                  and `RUN12 U7-SUFFICIENT|STRONGER-FORM|N/A`. A criterion whose fields the run
                  lacks is N/A, never PASS.

Exit codes: 0 ok (with --check: no FAIL), 1 a criterion FAILED, 2 usage or unreadable input.
Python 3.9+, standard library only.

The file has two parts: PART 1 (summary, table) does not depend on PART 2 (--check, windows,
criteria); the two are delivered as separate changes.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys

SCHEMA = 1
WARMUP_S = 60.0
HEAVY_DRAWS = 400

# Per-frame counter fields, in the order of the runtime's COUNT_NAMES (reserved_0 is never written).
COUNT_FIELDS = [
    "draw_total", "ei_draw_total", "dispatch_total", "ei_dispatch_total",
    "ei_draw_skip_building", "ei_draw_skip_late", "ei_draw_skip_abandoned",
    "ei_draw_skip_quarantined", "ei_draw_skip_failed", "ei_draw_skip_other",
    "draw_standin_building", "draw_standin_late", "draw_standin_abandoned",
    "draw_standin_quarantined", "draw_standin_failed", "draw_skip_other", "draw_pc",
    "dispatch_skip_building", "dispatch_skip_late", "dispatch_skip_image_only",
    "dispatch_skip_background", "dispatch_skip_size_gate", "dispatch_skip_abandoned",
    "dispatch_skip_quarantined", "dispatch_skip_failed", "dispatch_skip_other",
    "ei_after_skipped_dispatch", "mustdraw_skipped", "mustdraw_wait_timeout",
]
EI_DRAW_SKIP = [n for n in COUNT_FIELDS if n.startswith("ei_draw_skip_")]
STANDIN = [n for n in COUNT_FIELDS if n.startswith("draw_standin_")]
NO_OUTPUT = EI_DRAW_SKIP + STANDIN + ["draw_skip_other"]
DISPATCH_SKIP = [n for n in COUNT_FIELDS if n.startswith("dispatch_skip_")]
BUCKETS = ["0", "1-9", "10-99", "100-999", "1000+"]


class UsageError(Exception):
    pass


# ---------------------------------------------------------------- PART 1: reading and summary

def read_text(path):
    try:
        with open(path, "r", encoding="utf-8", errors="replace", newline="") as f:
            return f.read()
    except OSError as e:
        raise UsageError(f"cannot read {path}: {e}")


def read_json(path):
    """A dict, or None when the file is missing or not a JSON object."""
    if not os.path.isfile(path):
        return None
    try:
        v = json.loads(read_text(path).lstrip("﻿"))
    except (UsageError, ValueError):
        return None
    return v if isinstance(v, dict) else None


def read_events(path):
    """(events, malformed line count). Every event is a dict with a string `ev`."""
    out, bad = [], 0
    for line in read_text(path).splitlines():
        if not line.strip():
            continue
        try:
            e = json.loads(line)
        except ValueError:
            bad += 1
            continue
        if isinstance(e, dict) and isinstance(e.get("ev"), str):
            out.append(e)
        else:
            bad += 1
    return out, bad


def num(v):
    """v as a float when it is a JSON number (not a bool), else None."""
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        return None
    return float(v) if math.isfinite(v) else None


def tsec(e):
    t = num(e.get("t"))
    return None if t is None else t / 1000.0


def percentile(sorted_vals, p):
    """Nearest-rank percentile of a sorted list (p in 0..1)."""
    if not sorted_vals:
        return None
    return sorted_vals[max(0, math.ceil(p * len(sorted_vals)) - 1)]


def stats(vals):
    """{p50, p95, max, n} of numbers, or None when there are none."""
    v = sorted(vals)
    if not v:
        return None
    return {"p50": percentile(v, 0.5), "p95": percentile(v, 0.95), "max": v[-1], "n": len(v)}


def ratio(n, d):
    return None if not d else n / d


def group(items, keyf, valf):
    """{key: [values]} over items, skipping those whose value is None."""
    out = {}
    for it in items:
        v = valf(it)
        if v is not None:
            out.setdefault(keyf(it), []).append(v)
    return out


def by_ev(events, name):
    return [e for e in events if e["ev"] == name]


def find_scene_start(events, override):
    """(seconds or None, rule). The rule names which definition was used."""
    if override is not None:
        return override, "override"
    for e in by_ev(events, "phase"):
        if e.get("name") == "world" and e.get("source") in ("mark", "schedule") and tsec(e) is not None:
            return tsec(e), "mark"
    for e in by_ev(events, "frame"):
        d = num(e.get("draw_total"))
        if d is not None and d >= HEAVY_DRAWS and tsec(e) is not None:
            return tsec(e), "heavy_frame"
    return None, "none"


def sum_fields(frames, names):
    """(sums by name, set of names seen in at least one frame)."""
    sums, seen = {n: 0 for n in names}, set()
    for f in frames:
        for n in names:
            v = num(f.get(n))
            if v is not None:
                sums[n] += v
                seen.add(n)
    return sums, seen


def count_block(frames):
    """The `totals` / `post_warmup` object for a list of frame events: every count field summed,
    the grouped sums and the four ratios. Ratios are None when their fields are absent or zero."""
    sums, seen = sum_fields(frames, COUNT_FIELDS)
    wait = sum(num(f.get("mustdraw_wait_ms")) or 0.0 for f in frames)
    blk = {"frames": len(frames)}
    blk.update({n: int(v) for n, v in sums.items()})
    blk["mustdraw_wait_ms"] = round(wait, 3)
    ei_skip = sum(sums[n] for n in EI_DRAW_SKIP)
    standin = sum(sums[n] for n in STANDIN)
    no_out = sum(sums[n] for n in NO_OUTPUT)
    d_skip = sum(sums[n] for n in DISPATCH_SKIP)
    direct = sums["draw_total"] - sums["ei_draw_total"]
    blk.update({"ei_draw_skip": int(ei_skip), "draw_standin": int(standin), "draw_no_output": int(no_out),
                "dispatch_skip": int(d_skip), "direct_draws": int(direct)})
    have = lambda *n: all(x in seen for x in n)
    blk["ei_draw_skip_ratio"] = ratio(ei_skip, sums["ei_draw_total"]) if have("ei_draw_total") else None
    blk["draw_standin_ratio"] = ratio(standin, direct) if have("draw_total", "ei_draw_total") else None
    blk["draw_no_output_ratio"] = ratio(no_out, sums["draw_total"]) if have("draw_total") else None
    blk["dispatch_skip_ratio"] = ratio(d_skip, sums["dispatch_total"]) if have("dispatch_total") else None
    return blk


def dispatch_histogram(frames):
    """Dispatch skips per frame: {p50, p95, max, buckets}; None when no frame carries dispatch_total
    (the run predates the counters)."""
    if not any("dispatch_total" in f for f in frames):
        return None
    per = []
    for f in frames:
        per.append(sum(num(f.get(n)) or 0.0 for n in DISPATCH_SKIP))
    buckets = dict.fromkeys(BUCKETS, 0)
    for n in per:
        k = "0" if n < 1 else "1-9" if n < 10 else "10-99" if n < 100 else "100-999" if n < 1000 else "1000+"
        buckets[k] += 1
    s = sorted(per)
    return {"p50": percentile(s, 0.5), "p95": percentile(s, 0.95), "max": s[-1], "buckets": buckets}


def after(events, scene):
    return [e for e in events if scene is not None and (tsec(e) is not None) and tsec(e) >= scene + WARMUP_S]


def key_stats(d):
    """{key: stats} with '/'-joined string keys."""
    return {"/".join(str(x) for x in k) if isinstance(k, tuple) else str(k): stats(v) for k, v in sorted(d.items(), key=lambda kv: str(kv[0]))}


def summarize(events, manifest, report, scene_override, config, malformed):
    """The summary object (schema 1). Fields a run does not carry are None or empty."""
    scene, rule = find_scene_start(events, scene_override)
    frames = by_ev(events, "frame")
    pipes = by_ev(events, "pipeline")
    ready = by_ev(events, "pipeline_ready")
    unbuilt = by_ev(events, "pipeline_unbuilt")
    acts = by_ev(events, "activity")
    touts = by_ev(events, "compile_timeout")
    stage = lambda e: e.get("stage") if isinstance(e.get("stage"), str) else "?"
    cls = lambda e: e.get("class") if isinstance(e.get("class"), str) else "?"

    s = {"schema": SCHEMA, "run_id": manifest.get("run_id"), "adapter": manifest.get("adapter"),
         "config": config, "scene_start_s": scene, "scene_start_rule": rule}
    s["totals"] = count_block(frames)
    pw = [f for f in frames if scene is not None and tsec(f) is not None and tsec(f) >= scene + WARMUP_S]
    s["post_warmup"] = count_block(pw) if scene is not None else None
    s["dispatch_skips_per_frame"] = {"total": dispatch_histogram(frames),
                                     "post_warmup": dispatch_histogram(pw) if scene is not None else None}

    # pipelines per minute after the scene start, by stage x class
    ppm = None
    if scene is not None:
        ppm = {}
        for e in pipes:
            t = tsec(e)
            if t is None or t < scene:
                continue
            row = ppm.setdefault(f"{stage(e)}/{cls(e)}", [])
            m = int((t - scene) // 60)
            row.extend([0] * (m + 1 - len(row)))
            row[m] += 1
    s["pipelines_per_minute"] = ppm
    s["compile_ms"] = key_stats(group(pipes, lambda e: (stage(e), cls(e), str(e.get("blob"))), lambda e: num(e.get("compile_ms"))))
    s["dxil_bytes"] = key_stats(group(pipes, stage, lambda e: num(e.get("dxil_bytes"))))

    def qwait(e):
        if num(e.get("need_ms")) is None:
            return None
        q = num(e.get("queued_ms"))
        return None if q is None else q + (num(e.get("gate_ms")) or 0.0)
    s["queue_wait_ms"] = key_stats(group(ready, lambda e: (stage(e), "must_draw" if e.get("must_draw") is True else "normal"), qwait))
    held = group(ready, cls, lambda e: num(e.get("held_ms")))
    s["worker_held_ms"] = {k: ({"p95": v["p95"], "max": v["max"]} if v else None) for k, v in key_stats(held).items()}
    to = {}
    for e in touts:
        k = f"{e.get('kind')}/{cls(e)}"
        to[k] = to.get(k, 0) + 1
    s["compile_timeouts"] = to
    keys = {str(e.get("record")) for e in touts if e.get("quarantined") is True and e.get("record")}
    keys |= {str(e.get("key")) for e in ready if e.get("outcome") == "quarantined" and e.get("key")}
    s["quarantined_keys"] = sorted(keys)
    s["abandoned"] = sum(1 for e in ready if e.get("abandoned") is True) if ready else None
    hit = sum(num(a.get("cache_hit_layer")) or 0.0 for a in acts)
    miss = sum(num(a.get("cache_miss_layer")) or 0.0 for a in acts)
    s["cache_hit_layer_rate"] = ratio(hit, hit + miss) if any("cache_hit_layer" in a for a in acts) else None

    md = [e for e in ready if e.get("must_draw") is True]
    worst = max(md, key=lambda e: num(e.get("waited_ms")) or 0.0) if md else None
    s["mustdraw"] = {
        "skipped": s["totals"]["mustdraw_skipped"], "wait_timeout": s["totals"]["mustdraw_wait_timeout"],
        "wait_ms": s["totals"]["mustdraw_wait_ms"],
        "skipped_post_warmup": s["post_warmup"]["mustdraw_skipped"] if s["post_warmup"] else None,
        "pipelines": len(md) if ready else None,
        "largest_wait_ms": (num(worst.get("waited_ms")) if worst else None),
        "largest_wait_key": (worst.get("key") if worst else None)}

    bp = ((report.get("d3d12") or {}).get("background_pipelines") if isinstance(report.get("d3d12"), dict) else None)
    s["failed_builds"] = {"report_background_pipelines_failed": bp[1] if isinstance(bp, list) and len(bp) > 1 else None,
                          "pipeline_ready_failed": sum(1 for e in ready if e.get("outcome") == "failed") if ready else None}
    top = sorted(unbuilt, key=lambda e: -(num(e.get("skips")) or 0.0))[:20]
    s["unbuilt"] = {"count": len(unbuilt),
                    "top_by_skips": [{"key": e.get("key"), "stage": e.get("stage"), "class": e.get("class"),
                                      "skips": e.get("skips")} for e in top]}
    s["malformed_lines"] = malformed
    return s


def fmt(v):
    if v is None:
        return "-"
    if isinstance(v, float):
        return f"{v:.4g}" if abs(v) < 1e4 else f"{v:.0f}"
    return str(v)


def table(s):
    """The human-readable summary, one line per group."""
    def pct(r):
        return "-" if r is None else f"{r * 100:.3f}%"
    L = [f"run {fmt(s['run_id'])}  adapter {fmt(s['adapter'])}  config {fmt(s['config'])}  "
         f"scene start {fmt(s['scene_start_s'])} s ({s['scene_start_rule']})"]
    for label, b in (("totals", s["totals"]), ("post-warmup", s["post_warmup"])):
        if b is None:
            L.append(f"{label}: n/a (no scene start)")
            continue
        L.append(f"{label}: frames {b['frames']}  draws {b['draw_total']}  ei draws {b['ei_draw_total']}  "
                 f"ei skip {pct(b['ei_draw_skip_ratio'])}  standin {pct(b['draw_standin_ratio'])}  "
                 f"no-output {pct(b['draw_no_output_ratio'])}  dispatch skip {pct(b['dispatch_skip_ratio'])}")
    for label, h in s["dispatch_skips_per_frame"].items():
        L.append(f"dispatch skips per frame ({label}): " + ("n/a" if h is None else
                 f"p50 {fmt(h['p50'])} p95 {fmt(h['p95'])} max {fmt(h['max'])}  buckets {h['buckets']}"))
    for name in ("compile_ms", "dxil_bytes", "queue_wait_ms"):
        for k, v in s[name].items():
            L.append(f"{name} [{k}]: p50 {fmt(v['p50'])} p95 {fmt(v['p95'])} max {fmt(v['max'])} (n {v['n']})")
    for k, v in s["worker_held_ms"].items():
        L.append(f"worker_held_ms [{k}]: p95 {fmt(v['p95'])} max {fmt(v['max'])}")
    for k, v in (s["pipelines_per_minute"] or {}).items():
        L.append(f"pipelines per minute [{k}]: {v}")
    L.append(f"compile timeouts: {s['compile_timeouts'] or 'none'}  quarantined keys: {len(s['quarantined_keys'])}  "
             f"abandoned: {fmt(s['abandoned'])}  cache hit layer rate: {fmt(s['cache_hit_layer_rate'])}")
    m = s["mustdraw"]
    L.append(f"must-draw: skipped {m['skipped']}  wait timeouts {m['wait_timeout']}  wait {fmt(m['wait_ms'])} ms  "
             f"pipelines {fmt(m['pipelines'])}  largest wait {fmt(m['largest_wait_ms'])} ms")
    f = s["failed_builds"]
    L.append(f"failed builds: report {fmt(f['report_background_pipelines_failed'])}  "
             f"pipeline_ready {fmt(f['pipeline_ready_failed'])}  unbuilt {s['unbuilt']['count']}  "
             f"malformed lines {s['malformed_lines']}")
    return "\n".join(L)


# ---------------------------------------------------------------- PART 2: --check (acceptance criteria)

STILL_NAMES = {"boot", "intro", "loading", "menu", "cutscene", "splash", "credits"}
CLASS_HARD_MS = {("graphics", "S"): 60e3, ("graphics", "M"): 60e3, ("compute", "S"): 120e3, ("compute", "M"): 120e3,
                 ("graphics", "L"): 450e3, ("compute", "L"): 450e3, ("graphics", "XL"): 300e3, ("compute", "XL"): 300e3}
MUSTDRAW_HARD_MS = 10000.0
MUSTDRAW_SOFT_MS = 2000.0
QUEUE_WAIT_P95_MS = 100.0
AC5_WINDOW_S = 120.0
AC6_WINDOW_S = 600.0
AVAILABLE_MIN = 2
LANES_UNWANTED = (3, 4, 5)  # pso_q order WG, H, WC, UG, UC, S: UG + UC + S
SPAN_RE = re.compile(r"^\s*(\d+(?:\.\d+)?)\s*-\s*(\d+(?:\.\d+)?)\s*$")


def parse_span(text):
    m = SPAN_RE.match(text)
    if not m or float(m.group(1)) >= float(m.group(2)):
        raise UsageError(f"--still-span must be A-B seconds with A < B, got {text!r}")
    return float(m.group(1)), float(m.group(2))


def still_windows(events, spans):
    """Non-interactive windows in seconds: --still-span values, else the spans between marks whose
    name maps to Still (a mark lasts until the next mark, or the last event of the run)."""
    if spans:
        return list(spans)
    marks = [(tsec(e), e.get("name")) for e in by_ev(events, "phase")
             if e.get("source") in ("mark", "schedule") and tsec(e) is not None]
    end = max((tsec(e) for e in events if tsec(e) is not None), default=0.0)
    out = []
    for i, (t, name) in enumerate(marks):
        if name in STILL_NAMES:
            out.append((t, marks[i + 1][0] if i + 1 < len(marks) else end))
    return out


def in_windows(t, wins):
    return t is not None and any(a <= t <= b for a, b in wins)


def size_class(nbytes):
    return "S" if nbytes < 256 << 10 else "M" if nbytes < 768 << 10 else "L" if nbytes < 2 << 20 else "XL"


def verdict(parts):
    """Combine (ok or None, ...) parts: FAIL if any part fails; PASS only when every part could be
    evaluated; else N/A."""
    if any(p[0] is False for p in parts):
        return "FAIL"
    if any(p[0] is None for p in parts):
        return "N/A"
    return "PASS"


class Check:
    """One criterion line."""
    def __init__(self, name, status, measured, target, reason=""):
        self.name, self.status, self.measured, self.target, self.reason = name, status, measured, target, reason

    def line(self):
        r = f" ({self.reason})" if self.reason else ""
        return f"{self.name} {self.status} measured={self.measured} target={self.target}{r}"


def build(name, parts, target):
    """parts: list of (ok|None, measured text, reason text)."""
    st = verdict(parts)
    measured = " ".join(p[1] for p in parts if p[1])
    reasons = "; ".join(p[2] for p in parts if p[2] and (p[0] is None or st == "FAIL" and p[0] is False))
    return Check(name, st, measured or "-", target, reasons)


def pct(r):
    return f"{r * 100:.3f}%"


def ratio_part(label, num_, den, limit, present, absent_reason):
    if not present:
        return (None, "", absent_reason)
    if not den:
        return (None, "", f"{label}: no draws to divide by")
    r = num_ / den
    return (r <= limit, f"{label}={pct(r)}", f"{label} above {pct(limit)}")


def count_part(label, n, present, absent_reason):
    if not present:
        return (None, "", absent_reason)
    return (n == 0, f"{label}={int(n)}", f"{label} not zero")


def check_run(events, summary, config, spans, report):
    """All criteria as a list of Check, plus the RUN12 line text."""
    scene = summary["scene_start_s"]
    tot, pw = summary["totals"], summary["post_warmup"]
    frames = by_ev(events, "frame")
    ready, unbuilt, acts = by_ev(events, "pipeline_ready"), by_ev(events, "pipeline_unbuilt"), by_ev(events, "activity")
    has_ei = any("ei_draw_total" in f for f in frames)
    has_draw = any("draw_total" in f for f in frames)
    has_disp = any("dispatch_total" in f for f in frames)
    no_scene = "no scene start (post-warmup checks need one)"
    no_pw = "no frame after the 60 s warm-up"
    no_cfg = "--config not given"
    no_q2 = "fields not in this run (unit Q2)"
    warm = config in ("warm", "prewarmed")
    out = []

    def post_ok():
        return scene is not None and pw is not None and pw["frames"] > 0

    # AC1 / AC2: skipped indirect draws, stand-ins for direct draws
    for name, grp, den_t, present in (
            ("AC1", "ei_draw_skip", tot["ei_draw_total"], has_ei),
            ("AC2", "draw_standin", tot["direct_draws"], has_ei and has_draw)):
        if config is None:
            out.append(Check(name, "N/A", "-", "per --config", no_cfg))
        elif not present:
            out.append(Check(name, "N/A", "-", "0.1% (cold) / 0 post-warmup (warm)", no_q2))
        elif warm:
            part = (None, "", no_scene) if scene is None else (None, "", no_pw) if not post_ok() else \
                count_part(f"{grp}_post_warmup", pw[grp], True, "")
            out.append(build(name, [part], "0 post-warmup"))
        else:
            out.append(build(name, [ratio_part(f"{grp}_ratio", tot[grp], den_t, 0.001, True, "")], "<= 0.1% whole run"))

    # AC3: draws that produced no output
    p_whole = ratio_part("no_output_whole", tot["draw_no_output"], tot["draw_total"], 0.01, has_draw, no_q2)
    if scene is None:
        p_post = (None, "", no_scene)
    elif not post_ok():
        p_post = (None, "", no_pw)
    else:
        p_post = ratio_part("no_output_post_warmup", pw["draw_no_output"], pw["draw_total"], 0.001, has_draw, no_q2)
    out.append(build("AC3", [p_whole, p_post], "<= 1% whole run, <= 0.1% post-warmup"))

    # AC4: dispatch skips
    h = summary["dispatch_skips_per_frame"]["total"]
    if config is None:
        out.append(Check("AC4", "N/A", "-", "per --config", no_cfg))
    elif not has_disp:
        out.append(Check("AC4", "N/A", "-", "measured", no_q2))
    elif not warm:
        out.append(Check("AC4", "PASS", f"per-frame p50={fmt(h['p50'])} p95={fmt(h['p95'])} max={fmt(h['max'])}", "measured"))
    else:
        if scene is None or not post_ok():
            out.append(Check("AC4", "N/A", "-", "0 outside image_only post-warmup", no_scene if scene is None else no_pw))
        else:
            n = sum(pw[k] for k in DISPATCH_SKIP if k != "dispatch_skip_image_only")
            out.append(build("AC4", [(n == 0, f"dispatch_skips_not_image_only={n}", "dispatch skips outside image_only")],
                             "0 outside image_only post-warmup"))

    # AC5: graphics pipelines needed in the first 120 s are built by scene + 120 s; P0/P1 queue wait
    q9 = any("outcome" in e for e in ready) or bool(unbuilt)
    if scene is None:
        out.append(Check("AC5", "N/A", "-", "see AC5 rule", no_scene))
    elif not ready:
        out.append(Check("AC5", "N/A", "-", "see AC5 rule", "no pipeline_ready events (unit Q1/pool U3)"))
    else:
        hi = scene + AC5_WINDOW_S

        def needed_in_window(e):
            t, nm = tsec(e), num(e.get("need_ms"))
            return t is not None and nm is not None and scene <= t - nm / 1000.0 <= hi
        late = sum(1 for e in ready if e.get("stage") == "graphics" and needed_in_window(e) and tsec(e) > hi)
        miss = sum(1 for e in unbuilt if e.get("stage") == "graphics" and needed_in_window(e))
        p_late = (late == 0, f"built_late={late}", "graphics pipelines built after scene+120 s")
        p_unb = (miss == 0, f"unbuilt={miss}", "graphics pipelines still unbuilt") if q9 else \
            (None, "", "no pipeline_unbuilt data (unit Q9)")
        gate = lambda e: num(e.get("gate_ms")) or 0.0
        qs = sorted((num(e.get("queued_ms")) + gate(e)) for e in ready
                    if num(e.get("need_ms")) is not None and num(e.get("queued_ms")) is not None)
        if qs:
            p95 = percentile(qs, 0.95)
            p_q = (p95 <= QUEUE_WAIT_P95_MS, f"queue_wait_p95_ms={fmt(p95)}", "queue-wait p95 above 100 ms")
        else:
            p_q = (None, "", "no queued_ms on pipeline_ready (unit Q9)")
        out.append(build("AC5", [p_late, p_unb, p_q], "none late/unbuilt in scene+120 s; queue-wait p95 <= 100 ms"))

    # AC6: unwanted lanes (UG + UC + S) drain by scene + 600 s
    qsamples = [(tsec(a), a["pso_q"]) for a in acts if tsec(a) is not None and isinstance(a.get("pso_q"), list)
                and len(a["pso_q"]) > max(LANES_UNWANTED) and all(num(x) is not None for x in a["pso_q"])]
    if scene is None:
        out.append(Check("AC6", "N/A", "-", "UG+UC+S = 0 by scene+600 s", no_scene))
    elif not qsamples:
        out.append(Check("AC6", "N/A", "-", "UG+UC+S = 0 by scene+600 s", "no activity.pso_q (pool U3)"))
    else:
        lanes = [(t, sum(q[i] for i in LANES_UNWANTED)) for t, q in qsamples if t >= scene]
        zero = next((i for i, (t, v) in enumerate(lanes) if t <= scene + AC6_WINDOW_S and v == 0), None)
        if zero is None:
            if not lanes or lanes[-1][0] < scene + AC6_WINDOW_S:
                out.append(Check("AC6", "N/A", "-", "UG+UC+S = 0 by scene+600 s", "the run ended before scene+600 s without a zero"))
            else:
                out.append(Check("AC6", "FAIL", "never 0", "UG+UC+S = 0 by scene+600 s"))
        else:
            inc = sum(1 for i in range(zero + 1, len(lanes)) if lanes[i][1] > lanes[i - 1][1])
            out.append(Check("AC6", "PASS", f"zero_at_s={lanes[zero][0]:.1f} increases_after={inc}", "UG+UC+S = 0 by scene+600 s"))

    # AC7: (a) compile time within the hard limit of its class, (b) at least 2 workers available
    over, hard_ev = {}, {}
    evaluable_a = False
    for e in by_ev(events, "pipeline"):
        ms, st = num(e.get("compile_ms")), e.get("stage")
        c = e.get("class") if isinstance(e.get("class"), str) else (size_class(num(e.get("dxil_bytes"))) if num(e.get("dxil_bytes")) is not None else None)
        if ms is None or (st, c) not in CLASS_HARD_MS:
            continue
        evaluable_a = True
        if ms > CLASS_HARD_MS[(st, c)]:
            over[(st, c)] = over.get((st, c), 0) + 1
    for e in by_ev(events, "compile_timeout"):
        if e.get("kind") == "hard":
            k = (e.get("stage"), e.get("class"))
            hard_ev[k] = hard_ev.get(k, 0) + 1
    viol = sum(max(0, n - hard_ev.get(k, 0)) for k, n in over.items())
    p_a = (viol == 0, f"compiles_over_hard_limit_without_event={viol}", "a compile ran past its class limit without a hard timeout") \
        if evaluable_a else (None, "", "no pipeline events with compile_ms and a class or dxil_bytes")

    def workers(a):
        w = a.get("pso_workers")
        g = lambda k: num(w.get(k)) if isinstance(w, dict) else num(a.get(f"pso_workers_{k}"))
        vals = [g("reserved"), g("general"), g("parked"), num(a.get("pso_workers_abandoned")), num(a.get("pso_large_held"))]
        return None if any(v is None for v in vals) else vals
    avail = [(tsec(a), v[0] + v[1] - v[2] - v[3] - v[4]) for a in acts for v in [workers(a)] if v is not None and tsec(a) is not None]
    if avail:
        low = [x for x in avail if x[1] < AVAILABLE_MIN]
        p_b = (not low, f"min_available_workers={fmt(min(v for _, v in avail))} samples_below_2={len(low)}", "fewer than 2 workers available")
    else:
        p_b = (None, "", "no activity sample with pso_workers, pso_workers_abandoned and pso_large_held (units U3, Q9)")
    out.append(build("AC7", [p_a, p_b], "compile <= hard limit; available workers >= 2"))

    # AC8: must-draw pipelines in Still windows
    wins = still_windows(events, spans)
    if not wins:
        out.append(Check("AC8", "N/A", "-", "see AC8 rule", "no Still windows (no marks and no --still-span)"))
    else:
        skipped = sum(num(f.get("mustdraw_skipped")) or 0.0 for f in frames if in_windows(tsec(f), wins))
        p_s = count_part("mustdraw_skipped_in_still", skipped, has_ei, no_q2)

        def in_still(e):
            t, nm = tsec(e), num(e.get("need_ms"))
            return in_windows(t, wins) or (t is not None and nm is not None and in_windows(t - nm / 1000.0, wins))
        md = [num(e.get("waited_ms")) for e in ready if e.get("must_draw") is True and in_still(e) and num(e.get("waited_ms")) is not None]
        has_md = any("must_draw" in e for e in ready)
        if not has_md:
            p_w = (None, "", "no must_draw on pipeline_ready (unit Q9)")
        else:
            top = max(md) if md else 0.0
            share = (sum(1 for w in md if w <= MUSTDRAW_SOFT_MS) / len(md)) if md else 1.0
            p_w = (top <= MUSTDRAW_HARD_MS, f"max_waited_ms={fmt(top)} share_le_2s={share * 100:.1f}%", "a must-draw pipeline waited over 10 s")
        out.append(build("AC8", [p_s, p_w], "mustdraw_skipped 0 and waited_ms <= 10000 in Still windows (cards: person checks)"))

    # AC10: no failed builds
    bp = summary["failed_builds"]["report_background_pipelines_failed"]
    rf = summary["failed_builds"]["pipeline_ready_failed"]
    p_r = (None, "", "report.json d3d12.background_pipelines[1] missing") if bp is None else (bp == 0, f"report_failed={fmt(bp)}", "report.json counts failed builds")
    p_e = (None, "", "no pipeline_ready events") if rf is None else (rf == 0, f"pipeline_ready_failed={rf}", "a pipeline_ready has outcome failed")
    out.append(build("AC10", [p_r, p_e], "0 and 0"))

    # RUN12 rule: U7 sufficient?
    by_name = {c.name: c for c in out}
    p95s = [num(e.get("queued_ms")) + (num(e.get("gate_ms")) or 0.0) for e in ready
            if num(e.get("need_ms")) is not None and num(e.get("queued_ms")) is not None]
    run = 0
    long_held = None
    for a in acts:
        w = a.get("pso_workers")
        gen = num(w.get("general")) if isinstance(w, dict) else num(a.get("pso_workers_general"))
        lh = num(a.get("pso_large_held"))
        if gen is None or lh is None:
            continue
        long_held = False if long_held is None else long_held
        run = run + 1 if lh >= gen else 0
        if run >= 3:
            long_held = True
    post = []
    for name, grp, den, thr in (("AC1", "ei_draw_skip", "ei_draw_total", 0.001), ("AC2", "draw_standin", "direct_draws", 0.001)):
        if pw is None or config is None or not (has_ei and pw["frames"] > 0):
            post.append(None)
        elif warm:
            post.append(pw[grp] == 0)
        else:
            post.append(None if not pw[den] else pw[grp] / pw[den] <= thr)
    comps = [None if not p95s else percentile(sorted(p95s), 0.95) <= QUEUE_WAIT_P95_MS] + post + [None if long_held is None else not long_held]
    if any(c is False for c in comps):
        r12 = "STRONGER-FORM"
    elif any(c is None for c in comps):
        r12 = "N/A"
    else:
        r12 = "U7-SUFFICIENT"
    return out, r12


# ---------------------------------------------------------------- command line

def main(argv=None):
    ap = argparse.ArgumentParser(prog="pso-summary.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("run_dir")
    ap.add_argument("--scene-start", type=float, default=None, metavar="S")
    ap.add_argument("--config", choices=("cold", "warm", "prewarmed"), default=None)
    ap.add_argument("--still-span", action="append", default=[], metavar="A-B")  # PART 2
    ap.add_argument("--check", action="store_true")                              # PART 2
    ap.add_argument("--out", default=None, metavar="FILE")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args(argv)
    try:
        spans = [parse_span(s) for s in args.still_span]                         # PART 2
        run = args.run_dir
        if not os.path.isdir(run):
            raise UsageError(f"not a folder: {run}")
        events, bad = read_events(os.path.join(run, "events.jsonl"))
        manifest = read_json(os.path.join(run, "manifest.json")) or {}
        report = read_json(os.path.join(run, "report.json")) or {}
        summary = summarize(events, manifest, report, args.scene_start, args.config, bad)
        out = args.out or os.path.join(run, "summary.json")
        try:
            with open(out, "w", encoding="utf-8", newline="\n") as f:
                json.dump(summary, f, indent=2, sort_keys=True)
                f.write("\n")
        except OSError as e:
            raise UsageError(f"cannot write {out}: {e}")
    except UsageError as e:
        print(f"pso-summary: {e}", file=sys.stderr)
        return 2
    if not args.quiet:
        print(table(summary))
    if not args.check:                                                           # PART 2 (to the end of main)
        return 0
    checks, r12 = check_run(events, summary, args.config, spans, report)
    for c in checks:
        print(c.line())
    print(f"RUN12 {r12}")
    return 1 if any(c.status == "FAIL" for c in checks) else 0


if __name__ == "__main__":
    sys.exit(main())
