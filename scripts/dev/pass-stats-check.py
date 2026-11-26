#!/usr/bin/env python3
"""Check the `pass_stats` events of an armed frame against invariants and print the findings.

  pass-stats-check.py --run <run-dir> [--frame N]

Reads `events.jsonl` of the run folder (untrusted data: only parsed, never evaluated). Without
--frame every frame that has `pass_stats` events is checked, each on its own. Output, plain text:

  FINDING <rule> frame=F list=L pass=P res=0x... <detail>     (one per finding)
  SUMMARY frames=N snapshots=N findings=N <rule>=N ...         (one line)

Rules: nan_inf, unchanged, constant, all_zero, read_unwritten, first_constant_ancestor, skipped.
`constant` is not repeated for a snapshot that is `all_zero`. Snapshots with `skipped` set carry no
statistics: they are only counted by `skipped`.

Exit codes: 0 no finding, 1 findings, 2 usage or unreadable input.
Python 3.9+, standard library only.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import sys

RULES = ["nan_inf", "unchanged", "constant", "all_zero", "read_unwritten",
         "first_constant_ancestor", "skipped"]


class UsageError(Exception):
    pass


def read_events(path):
    """The `pass_stats` events of the file, in file order, as dicts. Bad lines are ignored."""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError as e:
        raise UsageError(f"cannot read {path}: {e}")
    out = []
    for line in text.splitlines():
        if not line.strip():
            continue
        try:
            e = json.loads(line)
        except ValueError:
            continue
        if isinstance(e, dict) and e.get("ev") == "pass_stats":
            out.append(e)
    return out


def num(v):
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        return None
    return float(v) if math.isfinite(v) else None


def is_constant(e):
    """min == max on all channels and more than one value."""
    mn, mx, cnt = e.get("min"), e.get("max"), num(e.get("count"))
    if not isinstance(mn, list) or not isinstance(mx, list) or not mn or len(mn) != len(mx):
        return False
    if cnt is None or cnt <= 1:
        return False
    return all(num(a) is not None and num(a) == num(b) for a, b in zip(mn, mx))


def is_all_zero(e):
    z, cnt = num(e.get("zero")), num(e.get("count"))
    return z is not None and cnt is not None and cnt > 0 and z == cnt


def key(e):
    return (e.get("list"), e.get("pass"))


def check_frame(frame, evs):
    """[(rule, event or None, detail)] for the events of one frame, in file order."""
    found = []
    skipped = {}
    live = []
    for e in evs:
        if e.get("skipped"):
            skipped[str(e["skipped"])] = skipped.get(str(e["skipped"]), 0) + 1
        else:
            live.append(e)
    for reason in sorted(skipped):
        found.append(("skipped", None, f"{skipped[reason]} snapshot(s) skipped: {reason}"))
    pre = {}
    for e in live:
        if e.get("when") == "pre":
            pre[(key(e), e.get("res"))] = e
    written = set()
    first_pass = key(live[0]) if live else None
    for e in live:
        if e.get("when") != "post":
            continue
        n = (num(e.get("nan")) or 0) + (num(e.get("inf")) or 0)
        if n > 0:
            found.append(("nan_inf", e, f"nan={e.get('nan')} inf={e.get('inf')}"))
        p = pre.get((key(e), e.get("res")))
        if p is not None and p.get("hash") is not None and p.get("hash") == e.get("hash"):
            found.append(("unchanged", e, f"hash={e.get('hash')} equals the pre snapshot"))
        zero = is_all_zero(e)
        if zero:
            found.append(("all_zero", e, f"zero={e.get('zero')} count={e.get('count')}"))
        elif is_constant(e):
            found.append(("constant", e, f"min=max={e.get('min')} count={e.get('count')}"))
        if key(e) != first_pass:
            for r in e.get("reads") or []:
                if r not in written:
                    found.append(("read_unwritten", e, f"reads {r}, no pass wrote it earlier in the frame"))
        written.add(e.get("res"))
    # first_constant_ancestor: the presented resource is the last `rt` post snapshot of the last list.
    posts = [e for e in live if e.get("when") == "post"]
    if posts:
        last_list = posts[-1].get("list")
        rts = [e for e in posts if e.get("list") == last_list and e.get("kind") == "rt"]
        if rts:
            res = rts[-1].get("res")
            for e in posts:
                if e.get("res") == res and (is_constant(e) or is_all_zero(e)):
                    found.append(("first_constant_ancestor", e,
                                  f"presented {res} first became constant after this pass"))
                    break
    return found


def check(events, frame=None):
    frames = {}
    for e in events:
        if frame is None or e.get("frame") == frame:
            frames.setdefault(e.get("frame"), []).append(e)
    lines, counts, total = [], {r: 0 for r in RULES}, 0
    for fr in frames:
        for rule, e, detail in check_frame(fr, frames[fr]):
            counts[rule] += 1
            total += 1
            if e is None:
                lines.append(f"FINDING {rule} frame={fr} list=- pass=- res=- {detail}")
            else:
                lines.append(f"FINDING {rule} frame={fr} list={e.get('list')} pass={e.get('pass')} "
                             f"res={e.get('res')} {detail}")
    nsnap = sum(len(v) for v in frames.values())
    per = " ".join(f"{r}={counts[r]}" for r in RULES if counts[r])
    lines.append(f"SUMMARY frames={len(frames)} snapshots={nsnap} findings={total}" + (" " + per if per else ""))
    return lines, total


def main(argv=None):
    ap = argparse.ArgumentParser(prog="pass-stats-check.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--run", required=True, metavar="DIR")
    ap.add_argument("--frame", type=int, default=None, metavar="N")
    args = ap.parse_args(argv)
    try:
        if not os.path.isdir(args.run):
            raise UsageError(f"not a folder: {args.run}")
        events = read_events(os.path.join(args.run, "events.jsonl"))
    except UsageError as e:
        print(f"pass-stats-check: {e}", file=sys.stderr)
        return 2
    lines, total = check(events, args.frame)
    print("\n".join(lines))
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main())
