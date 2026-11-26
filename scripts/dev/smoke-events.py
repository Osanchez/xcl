#!/usr/bin/env python3
"""Print the `xcl-smoke v1` block of a finished run, taking each figure from the run's
`events.jsonl` when an event carries it and from the log otherwise.

  smoke-events.py --log <xcl.log> [--run <run-dir>] (--game <folder> | --title <label>)
                  [--seconds N] [--repo <xcl checkout>] [--explain]

The block is the one `scripts/smoke.ps1` prints (same 12 lines, labels and spacing, `\\n` line
ends), so it can be pasted into a PR's "Regression run" section. stdout carries only the block;
`--explain` writes the source of each line (events / log / report / git), the manifest's
`compare_key` and the number of malformed events lines to stderr.

  --run      run folder (events.jsonl, manifest.json, report.json). Default: the newest `run-*`
             folder next to the log whose manifest `log` field names this log (paths compared
             case- and separator-insensitively). None found: log-only mode (a build without events).
  --game     the title is the first 4 bytes of the SHA-256 of the folder's trimmed leaf name
             (UTF-8) as 8 hex digits, as in smoke.ps1. --title prints the given label instead.
  --seconds  default: the manifest's switches.XCL_RUN_SECONDS (an error when neither is known).
  --repo     checkout whose HEAD is reported (default: the repository containing this script).

Known divergences from smoke.ps1:
  1. A run smoke.ps1 killed on its own timeout prints `killed (no exit)`; this reader prints the
     same only when neither an `exit` event nor a log end line exists.
  2. If the final XCL_ACTIVITY iteration was cut between its two log lines, smoke.ps1 reads one more
     `presents` sample than there are `activity` events (the event is written after the memory
     line). Both are rare; --explain names the source of each line.
Also: `gpu` is read from report.json in the run folder, then next to the log (smoke.ps1's place),
then the manifest's `adapter`; `device removals` always comes from the log.

All input files are untrusted data: they are only parsed (json, regex), never evaluated, and no
command line is built from them. Exit codes: 0 ok, 2 usage or unreadable input.
Python 3.9+, standard library only.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import unicodedata

FRAMES_RE = re.compile(r"\((\d+) frames shown, (\d+) PresentX calls\)", re.I)  # PowerShell -match is case-insensitive
PRESENTS_RE = re.compile(r"activity \(2 s\): (\d+) presents")
COMMIT_RE = re.compile(r"memory: (\d+) MB committed")
REMOVED_RE = re.compile(r"device removed")
CRASH_RE = re.compile(r"unhandled exception|process ends with status")
EXIT_STATUS_RE = re.compile(r"process ends with status (0[xX][0-9a-fA-F]+)")
EXIT_PLAIN_RE = re.compile(r"process ends \(thread")
VRAM_RE = re.compile(r"\s*\([^()]*VRAM\)$")
CIM_COMMAND = ("(Get-CimInstance Win32_VideoController | Where-Object { $_.Name -notmatch 'Basic' } "
               "| Select-Object -First 1).Name")
NO_FRAMES = "none logged (the run did not reach its end)"


class UsageError(Exception):
    pass


def norm_path(p):
    return str(p).replace("\\", "/").lower()


def read_text(path):
    try:
        with open(path, "r", encoding="utf-8-sig", errors="replace", newline="") as f:
            return f.read()
    except OSError as e:
        raise UsageError(f"cannot read {path}: {e}")


def read_json(path):
    try:
        v = json.loads(read_text(path))
    except (UsageError, ValueError):
        return None
    return v if isinstance(v, dict) else None


def title_from(game=None, title=None):
    """smoke.ps1:21-30."""
    if title is not None:
        t = title.strip()
        if not t or "`" in t or any(unicodedata.category(c) in ("Cc", "Cf", "Zl", "Zp") for c in t):
            raise UsageError("--title must be one line of text without backticks")
        return t
    s = game.rstrip("/\\")
    leaf = re.split(r"[/\\]", s)[-1] if s else ""
    return hashlib.sha256(leaf.strip().encode("utf-8")).digest()[:4].hex()


def find_run(log):
    """Newest `run-*` folder next to the log whose manifest `log` names it, or None."""
    base = os.path.dirname(os.path.abspath(log))
    want = norm_path(os.path.abspath(log))
    best = None
    try:
        names = os.listdir(base)
    except OSError:
        return None
    for n in names:
        d = os.path.join(base, n)
        if not (n.startswith("run-") and os.path.isdir(d)):
            continue
        m = read_json(os.path.join(d, "manifest.json"))
        if not m or not isinstance(m.get("log"), str):
            continue
        ml = m["log"]
        if norm_path(ml) != want and norm_path(os.path.abspath(ml)) != want:
            continue
        key = (os.path.getmtime(d), n)
        if best is None or key > best[0]:
            best = (key, d)
    return best[1] if best else None


def read_events(path):
    """(events, malformed line count). A missing file gives ([], 0)."""
    if not os.path.isfile(path):
        return [], 0
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


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def run_git(repo, *args):
    try:
        r = subprocess.run(["git", "-C", repo] + list(args), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           universal_newlines=True, check=False)
    except OSError as e:
        raise UsageError(f"git failed: {e}")
    if r.returncode != 0:
        raise UsageError(f"git {' '.join(args)} failed: {r.stderr.strip()}")
    return r.stdout


def cim_adapter():
    try:
        r = subprocess.run(["powershell", "-NoProfile", "-Command", CIM_COMMAND], stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, universal_newlines=True, check=False)
    except OSError:
        return "unknown"
    return r.stdout.strip() if r.returncode == 0 else "unknown"


def gpu_name(run, log, manifest, src):
    paths = ([os.path.join(run, "report.json")] if run else []) + [os.path.join(os.path.dirname(os.path.abspath(log)), "report.json")]
    for p in paths:
        rep = read_json(p) if os.path.isfile(p) else None
        d3d = rep.get("d3d12") if rep else None
        a = d3d.get("adapter") if isinstance(d3d, dict) else None
        if isinstance(a, str) and VRAM_RE.sub("", a):
            src["gpu"] = "report"
            return VRAM_RE.sub("", a)
    a = manifest.get("adapter")
    if isinstance(a, str) and a:
        src["gpu"] = "report (manifest adapter)"
        return a
    src["gpu"] = "log (CIM query)" if sys.platform == "win32" else "none"
    return cim_adapter() if sys.platform == "win32" else "unknown"


def build_block(log, run, title, seconds, repo, manifest=None, events=(), src=None):
    """The 12 lines of the block, as a list. `src` receives each line's source."""
    src = {} if src is None else src
    manifest = manifest or {}
    text = read_text(log)
    last = lambda name: [e for e in events if e["ev"] == name]

    commit = run_git(repo, "rev-parse", "HEAD").strip()
    dirty = bool(run_git(repo, "status", "--porcelain", "--untracked-files=no", "--", "crates", "Cargo.toml", "Cargo.lock").strip())
    src["commit"] = "git"

    exits = [e for e in last("exit") if is_int(e.get("status"))]
    if exits:
        status = exits[-1]["status"]
        code = "0x{:x}".format(status & 0xFFFFFFFF)
        src["exit"] = "events"
    else:
        ms = EXIT_STATUS_RE.findall(text)
        src["exit"] = "log"
        if ms:
            code = "0x{:x}".format(int(ms[-1], 16))
        elif EXIT_PLAIN_RE.search(text):
            code = "0x0"
        else:
            code = "killed (no exit)"

    ends = [e for e in last("end") if e.get("reason") == "run_seconds" and is_int(e.get("frames_shown")) and is_int(e.get("presentx"))]
    if ends:
        frames = "{} (PresentX {})".format(ends[0]["frames_shown"], ends[0]["presentx"])
        src["frames shown"] = "events"
    else:
        m = FRAMES_RE.search(text)
        frames = "{} (PresentX {})".format(m.group(1), m.group(2)) if m else NO_FRAMES
        src["frames shown"] = "log"

    acts = last("activity")
    pres = [e["presents_2s"] for e in acts if is_int(e.get("presents_2s"))]
    if pres:
        src["presents per 2 s (last 3)"] = "events"
    else:
        pres = [int(x) for x in PRESENTS_RE.findall(text)]
        src["presents per 2 s (last 3)"] = "log"
    presents = " ".join(str(x) for x in pres[-3:])

    removals = len(REMOVED_RE.findall(text))
    src["device removals"] = "log"

    if exits:
        crashes = sum(1 for e in last("error") if e.get("kind") == "title_exception") + (1 if exits[-1]["status"] != 0 else 0)
        src["crashes"] = "events"
    else:
        crashes = len(CRASH_RE.findall(text))
        src["crashes"] = "log"

    mb = [e["committed_mb"] for e in acts if is_int(e.get("committed_mb"))]
    if mb:
        peak = str(max(mb))
        src["peak commit"] = "events"
    else:
        c = [int(x) for x in COMMIT_RE.findall(text)]
        peak = str(max(c)) if c else "n/a"
        src["peak commit"] = "log"

    gpu = gpu_name(run, log, manifest, src)
    src["title"] = "parameter"
    src["seconds"] = "parameter or manifest"
    return [
        "```",
        "xcl-smoke v1",
        "commit: " + commit + (" (uncommitted changes)" if dirty else ""),
        "gpu: " + gpu,
        "title: " + title,
        "seconds: " + str(seconds),
        "exit: " + code,
        "frames shown: " + frames,
        "presents per 2 s (last 3): " + presents,
        "device removals: " + str(removals),
        "crashes: " + str(crashes),
        "peak commit: " + peak + " MB",
        "```",
    ]


def main(argv=None):
    ap = argparse.ArgumentParser(description="Print the xcl-smoke v1 block of a finished run (events first, log as fallback).")
    ap.add_argument("--log", required=True, help="the run's xcl.log")
    ap.add_argument("--run", help="run folder (default: the newest run-* next to the log naming this log)")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--game", help="game folder: the title line is a hash of its name")
    g.add_argument("--title", help="label printed on the title line")
    ap.add_argument("--seconds", type=int, help="default: the manifest's switches.XCL_RUN_SECONDS")
    ap.add_argument("--repo", default=os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
                    help="checkout whose HEAD is reported (default: the one containing this script)")
    ap.add_argument("--explain", action="store_true", help="source of each line on stderr")
    a = ap.parse_args(argv)
    try:
        title = title_from(a.game, a.title)
        if not os.path.isfile(a.log):
            raise UsageError(f"no log at {a.log}")
        run = a.run or find_run(a.log)
        if run and not os.path.isdir(run):
            raise UsageError(f"no run folder at {run}")
        manifest = (read_json(os.path.join(run, "manifest.json")) or {}) if run else {}
        events, bad = read_events(os.path.join(run, "events.jsonl")) if run else ([], 0)
        seconds = a.seconds
        if seconds is None:
            sw = manifest.get("switches")
            v = sw.get("XCL_RUN_SECONDS") if isinstance(sw, dict) else None
            try:
                seconds = int(v)
            except (TypeError, ValueError):
                raise UsageError("--seconds is required: the manifest has no switches.XCL_RUN_SECONDS")
        src = {}
        lines = build_block(a.log, run, title, seconds, a.repo, manifest, events, src)
    except UsageError as e:
        sys.stderr.write(f"smoke-events: {e}\n")
        return 2
    sys.stdout.buffer.write(("\n".join(lines) + "\n").encode("utf-8"))
    if a.explain:
        out = [f"run folder: {run or '(none: log-only mode)'}", f"events: {len(events)} used, {bad} malformed line(s) skipped",
               f"compare_key: {manifest.get('compare_key', '')!r}"]
        out += [f"{k}: {v}" for k, v in src.items()]
        sys.stderr.write("\n".join(out) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
