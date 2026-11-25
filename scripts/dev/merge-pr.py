#!/usr/bin/env python3
"""The only sanctioned way to bring outside code (a pull request, upstream main) into this tree.

    merge-pr.py 1193 --approve <full 40-hex id>             merge upstream PR 1193
    merge-pr.py 1193 --repo OWNER/NAME --approve <id>       another repository
    merge-pr.py --sync --approve <id>                       merge upstream main
    merge-pr.py 1193 --approve <id> --dry-run               check everything, merge nothing

It merges only when all of this holds (any failure stops before HEAD is touched):

  * the working tree is clean;
  * `pr-gate.py status <id>` passes: the full diff was scanned and a review that covers every
    line is recorded for exactly this commit;
  * the pull request's (or upstream main's) head right now is <id>, and --approve is <id>:
    the user approved the commit that was reviewed, not whatever was pushed since;
  * merge-base(HEAD, <id>) is still the base the review was made against. If HEAD moved so
    that the diff would differ, scan and review again.

The merge is `--no-ff` (done as --no-commit, then commit: see .githooks/pre-merge-commit) with a
message naming the id, the review hash and the reviewer. There is no cherry-pick and no
no-commit option. <id>.approved.json is written just before the merge and is what the hook
checks; it is removed again if the merge fails without conflicts. Nothing here builds or tests the result.

Claude Code asks the user before it runs this script (scripts/dev/merge-guard.py).
"""
from __future__ import annotations

import argparse
import importlib.util
import os
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
ID_RE = re.compile(r"^[0-9a-f]{40}$")


def load_gate():
    """pr-gate.py has a hyphen in its name, so load it by path (stdlib only, no sys.path)."""
    path = os.path.join(HERE, "pr-gate.py")
    spec = importlib.util.spec_from_file_location("pr_gate", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"merge-pr: cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


gate = load_gate()


def say(*args: object, file=None) -> None:
    print(*args, file=file or sys.stdout, flush=True)


def die(msg: str, code: int = 1) -> "None":
    say(f"\nmerge-pr: {msg}", file=sys.stderr)
    sys.exit(code)


def git(*args: str) -> subprocess.CompletedProcess:
    try:
        return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True,
                              errors="replace", timeout=600)
    except (OSError, subprocess.SubprocessError) as e:
        die(f"`git {args[0]}` could not run: {e}", 3)


def main(argv: "list | None" = None) -> int:
    ap = argparse.ArgumentParser(description="Merge reviewed outside code into this tree.")
    ap.add_argument("number", nargs="?", type=int, help="pull request number")
    ap.add_argument("--sync", action="store_true", help="merge upstream main")
    ap.add_argument("--repo", default=gate.DEFAULT_REPO, help="OWNER/NAME")
    ap.add_argument("--approve", required=True, metavar="ID",
                    help="the full 40-hex commit id the user approves")
    ap.add_argument("--dry-run", action="store_true", help="check, but do not merge")
    a = ap.parse_args(argv)
    if (a.number is None) == (not a.sync):
        ap.error("give a pull request number or --sync, not both")
    if not ID_RE.match(a.approve):
        ap.error("--approve needs the full 40-hex commit id")
    commit = a.approve
    what = "upstream main" if a.sync else f"{a.repo}#{a.number}"

    status = git("status", "--porcelain")
    if status.returncode != 0 or status.stdout.strip():
        die("the working tree is not clean (or cannot be read). Commit or stash first: a merge "
            "on top of uncommitted work hides what came from outside.")

    head = gate.current_head(a.repo, a.number, a.sync)
    if head != commit:
        die(f"{what} is now at {head}, not the approved {commit}.\n"
            "It moved after the review: scan and review the new head, then approve that id.", 2)

    problems, scan, review = gate.verify(commit)
    if problems:
        die("the scan/review is not ready for this commit:\n  - " + "\n  - ".join(problems), 2)
    if scan.get("repo") != a.repo or scan.get("number") != a.number or bool(scan.get("sync")) != a.sync:
        die("the scan on record is for a different pull request or repository; scan again.", 2)
    if git("cat-file", "-e", f"{commit}^{{commit}}").returncode != 0:
        die(f"commit {commit} is not in this repository; scan again.", 2)
    base = git("merge-base", "HEAD", commit).stdout.strip()
    if base != scan["base"]:
        die(f"HEAD moved: the merge base is now {base or 'none'}, but the review was made "
            f"against {scan['base']}. Scan and review again.", 2)

    digest = gate.review_hash(commit)
    say(f"merge-pr: {what} at {commit}\n"
        f"  reviewed by {review['reviewer']} ({review['verdict']}), review sha256 {digest}")
    if a.dry_run:
        say("--dry-run: everything checks out. Nothing was merged.")
        return 0

    before = git("rev-parse", "HEAD").stdout.strip()
    approved = gate.state_path(commit, "approved.json")
    gate.write_json(approved, {
        "id": commit, "head": before, "base": scan["base"], "review_sha256": digest,
        "reviewer": review["reviewer"], "approved_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
    message = (f"Merge {what} ({commit[:12]}), reviewed\n\n"
               f"Commit:   {commit}\nReviewer: {review['reviewer']} ({review['verdict']})\n"
               f"Review:   sha256 {digest}\n")
    # --no-commit, then commit: during a plain `git merge` git runs pre-merge-commit before it
    # writes MERGE_HEAD, so that hook cannot tell this merge from any other (see the hook).
    r = git("merge", "--no-ff", "--no-commit", commit)
    say((r.stdout or "") + (r.stderr or ""), file=sys.stdout)
    in_progress = git("rev-parse", "-q", "--verify", "MERGE_HEAD").returncode == 0
    if r.returncode != 0 and in_progress:
        say(f"merge-pr: the merge stopped (exit {r.returncode}); resolve the conflicts and "
            "commit, or `git merge --abort`. The approval stays valid for this HEAD.")
        return r.returncode
    if r.returncode == 0 and in_progress:
        c = git("commit", "-m", message)
        say((c.stdout or "") + (c.stderr or ""), file=sys.stdout)
        if c.returncode == 0:
            say(f"\nmerge-pr: {what} ({commit[:12]}) is in. Nothing was built or tested; "
                "do that now.")
            return 0
        git("merge", "--abort")
        r = c
    try:
        os.remove(approved)
    except OSError:
        pass
    if r.returncode == 0:
        say("merge-pr: nothing to merge (already up to date); the approval was removed.")
        return 0
    say(f"merge-pr: the merge failed (exit {r.returncode}); the approval was removed.")
    return r.returncode


if __name__ == "__main__":
    sys.exit(main())
