#!/usr/bin/env python3
"""The only sanctioned way to merge a public pull request into this fork.

    scripts/dev/merge-pr.py 1193                       # gate, then merge upstream PR 1193
    scripts/dev/merge-pr.py 1193 --ack scripts/gen_rdna2.py   # acks pass through to the gate
    scripts/dev/merge-pr.py 1193 --cherry-pick         # take its commits instead of merging
    scripts/dev/merge-pr.py 1193 --no-commit           # merge into the index, do not commit
    scripts/dev/merge-pr.py 1193 --dry-run             # gate and fetch only, never touch HEAD

Why a wrapper instead of a git hook: `.git/hooks` is not committed, `--no-verify` bypasses it,
and `pre-merge-commit` never fires on a fast-forward merge or on `git cherry-pick`. Here the
security gate and the merge are one command, so there is no separate step to forget.

What it does, in order - it stops at the first failure and leaves HEAD untouched:

  1. reads the pull request's current head commit from the API;
  2. runs scripts/dev/pr-gate.py, which neither checks out nor runs any of its code;
  3. requires a gate receipt for *that exact commit* (a force-pushed pull request therefore
     has to pass the gate again);
  4. fetches `pull/<N>/head` and checks that what arrived is the commit that was reviewed -
     this is the window where a push could otherwise slip past the review;
  5. merges with --no-ff, so there is always a merge commit naming what came in.

Nothing here builds or tests the merged tree. Do that afterwards, deliberately.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
GATE = os.path.join(HERE, "pr-gate.py")
RECEIPTS = os.path.join(ROOT, ".git", "pr-gate")


def die(msg: str, code: int = 1) -> "None":
    say(f"\nmerge-pr: {msg}", file=sys.stderr)
    sys.exit(code)


def say(*args: object, file=None) -> None:
    """Print and flush. Child processes (the gate, git) write straight to the same console, so
    anything buffered here would appear after their output and the transcript would read in the
    wrong order - which matters when the transcript is the thing being read."""
    print(*args, file=file or sys.stdout, flush=True)


def git(*args: str, check: bool = True, quiet: bool = False) -> subprocess.CompletedProcess:
    cmd = ["git", "-C", ROOT, *args]
    if not quiet:
        say("  " + " ".join(cmd[3:] if cmd[1] == "-C" else cmd))
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
    if check and r.returncode != 0:
        detail = (r.stderr or r.stdout or "").strip()
        die(f"`git {' '.join(args)}` failed:\n{detail}")
    return r


def gh_head(repo: str, number: int) -> str:
    if not shutil.which("gh"):
        die("the GitHub CLI `gh` is not on PATH.", 3)
    r = subprocess.run(["gh", "api", f"repos/{repo}/pulls/{number}",
                        "--jq", ".head.sha"], capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        die(f"could not read the head commit of {repo}#{number}: "
            f"{(r.stderr or '').strip()}", 3)
    head = r.stdout.strip()
    if len(head) != 40:
        die(f"unexpected head commit for {repo}#{number}: {head!r}", 3)
    return head


def tree_is_clean() -> bool:
    return git("status", "--porcelain", quiet=True).stdout.strip() == ""


def main() -> int:
    ap = argparse.ArgumentParser(description="Gate, then merge a public pull request.")
    ap.add_argument("number", type=int)
    ap.add_argument("--repo", default="ivall/xcl", help="OWNER/NAME (default ivall/xcl)")
    ap.add_argument("--ack", action="append", default=[], metavar="PATH",
                    help="passed to pr-gate.py: a blocking path you have read and accepted")
    ap.add_argument("--cherry-pick", action="store_true",
                    help="cherry-pick the pull request's commits instead of merging it")
    ap.add_argument("--no-commit", action="store_true",
                    help="merge into the working tree and index without committing")
    ap.add_argument("--dry-run", action="store_true",
                    help="gate and fetch only; HEAD, the index and the working tree are untouched")
    a = ap.parse_args()

    if not os.path.isfile(GATE):
        die(f"the security gate is missing: {GATE}", 3)
    if not a.dry_run and not tree_is_clean():
        die("the working tree is not clean. Commit or stash first - a merge on top of "
            "uncommitted work makes it impossible to tell afterwards what came from the "
            "pull request.")

    branch = git("branch", "--show-current", quiet=True).stdout.strip() or "(detached HEAD)"
    say(f"merge-pr: {a.repo}#{a.number} into {branch}\n")

    # 1. the commit that will be reviewed, and 2. the gate itself.
    head = gh_head(a.repo, a.number)
    say(f"head commit: {head}\n")
    say("running the security gate (nothing of the pull request is checked out or run)...\n")
    gate = subprocess.run([sys.executable, "-I", GATE, str(a.number), "--repo", a.repo,
                           *[x for p in a.ack for x in ("--ack", p)]])
    if gate.returncode != 0:
        die(f"the security gate exited {gate.returncode}; nothing was fetched or merged.\n"
            "Read the paths it listed, then re-run with one --ack per path you accept.",
            gate.returncode)

    # 3. the receipt must be for this exact commit.
    receipt = os.path.join(RECEIPTS, f"{head}.json")
    if not os.path.isfile(receipt):
        die(f"the gate passed but left no receipt for {head}.\nExpected {receipt}. "
            "Refusing to merge an unreceipted commit.", 3)
    try:
        with open(receipt, encoding="utf-8") as f:
            doc = json.load(f)
    except (OSError, ValueError) as e:
        die(f"the receipt {receipt} could not be read ({e}); refusing to merge.", 3)
    if doc.get("verdict") != "CLEAR" or doc.get("head") != head:
        die(f"the receipt {receipt} does not clear {head}; refusing to merge.", 3)
    if doc.get("acknowledged"):
        say("\nmanually acknowledged in the review:")
        for p in doc["acknowledged"]:
            say(f"    - {p}")

    # 4. fetch, and check that what arrived is what was reviewed.
    say(f"\nfetching pull/{a.number}/head ...")
    git("fetch", "--no-tags", f"https://github.com/{a.repo}", f"pull/{a.number}/head")
    fetched = git("rev-parse", "FETCH_HEAD", quiet=True).stdout.strip()
    if fetched != head:
        die(f"what was fetched ({fetched}) is not what the gate reviewed ({head}).\n"
            "The pull request was pushed to between the scan and the fetch. Run this again "
            "so the new commit is reviewed.", 2)
    say(f"fetched {fetched} - matches the reviewed commit")

    if a.dry_run:
        say("\n--dry-run: gate passed and the commit is verified. Nothing was merged.")
        say(f"To merge it: python scripts/dev/merge-pr.py {a.number} --repo {a.repo}")
        return 0

    # 5. merge (or cherry-pick), always leaving a record of what came in.
    say()
    if a.cherry_pick:
        base = git("merge-base", "HEAD", "FETCH_HEAD", quiet=True).stdout.strip()
        if not base:
            die("no common ancestor with the pull request; cannot cherry-pick a range.")
        r = git("cherry-pick", f"{base}..FETCH_HEAD", check=False)
    else:
        args = ["merge", "--no-ff", "FETCH_HEAD", "-m",
                f"Merge {a.repo}#{a.number} ({head[:12]}), security gate passed"]
        if a.no_commit:
            args = ["merge", "--no-ff", "--no-commit", "FETCH_HEAD"]
        r = git(*args, check=False)
    say((r.stdout or "") + (r.stderr or ""), end="")
    if r.returncode != 0:
        say(f"\nmerge-pr: the merge did not complete (exit {r.returncode}).")
        say("Resolve the conflicts and commit, or `git merge --abort` to undo it.")
        say("The gate already passed for this commit, so no re-review is needed.")
        return r.returncode

    say(f"\nmerge-pr: {a.repo}#{a.number} ({head[:12]}) is in, gate passed.")
    say("Nothing was built or tested. Do that now, deliberately:")
    say("    cargo build --workspace && cargo test --workspace -- --test-threads=4")
    return 0


if __name__ == "__main__":
    sys.exit(main())
