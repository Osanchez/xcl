#!/usr/bin/env python3
"""PreToolUse hook: refuse any merge that did not go through scripts/dev/merge-pr.py.

Wired up in .claude/settings.json. Claude Code hands it the tool call as JSON on stdin; exit 0
allows the call, exit 2 blocks it and shows this script's stderr to the agent.

The point is structural rather than advisory: the agent cannot run `git merge` at all, so it
cannot merge a public pull request without the security gate having passed. A git hook cannot
do this job - `.git/hooks` is not committed, `--no-verify` bypasses it, and `pre-merge-commit`
never fires on a fast-forward merge or on `git cherry-pick`.

Deliberately NOT blocked, because they are routine and safe:

  * `git merge --abort` / `--continue` / `--quit`, and the same for cherry-pick - recovery
  * `git merge-base`, `git merge-tree` - read-only
  * `git pull --ff-only` - the fork guide's own way to catch up on origin/dev
  * `git rebase` of your own work

If this ever gets in the way of legitimate work, widen ALLOW_SUFFIXES rather than removing the
hook; a guard that gets switched off protects nothing.
"""
from __future__ import annotations

import json
import os
import re
import shlex
import sys

# (command, subcommand) pairs that perform or publish a merge.
BLOCKED = {
    "git": {"merge", "cherry-pick", "pull", "am", "apply", "revert"},
    "gh": {"pr"},          # only `gh pr merge`, narrowed below
}
# Flags that make an otherwise blocked invocation safe.
ALLOW_SUFFIXES = {
    "merge": {"--abort", "--continue", "--quit"},
    "cherry-pick": {"--abort", "--continue", "--quit", "--skip"},
    "revert": {"--abort", "--continue", "--quit"},
    "pull": {"--ff-only"},
    "am": {"--abort", "--continue", "--quit", "--skip"},
}
SANCTIONED = re.compile(r"merge-pr\.py")
SPLIT = re.compile(r"&&|\|\||;|\|")


def segments(command: str) -> list:
    """The command split on shell separators, so `cd x && git merge y` is still seen."""
    return [s.strip() for s in SPLIT.split(command) if s.strip()]


def offending(command: str) -> "str | None":
    """The blocked invocation in this command, or None."""
    if SANCTIONED.search(command):
        return None                      # the wrapper is the sanctioned path; it runs the gate
    for segment in segments(command):
        try:
            tokens = shlex.split(segment)
        except ValueError:
            tokens = segment.split()
        if not tokens:
            continue
        # Skip an env prefix such as `PATH=... git merge`.
        while tokens and "=" in tokens[0] and not tokens[0].startswith("-"):
            tokens = tokens[1:]
        if not tokens:
            continue
        exe = os.path.basename(tokens[0]).lower()
        exe = exe[:-4] if exe.endswith(".exe") else exe
        if exe not in BLOCKED:
            continue
        # The first token that is not an option is the subcommand (`git -C dir merge`).
        sub, rest = None, []
        skip_value = {"-C", "-c", "--git-dir", "--work-tree", "--repo", "-R"}
        i = 1
        while i < len(tokens):
            t = tokens[i]
            if t in skip_value:
                i += 2
                continue
            if t.startswith("-"):
                i += 1
                continue
            sub, rest = t, tokens[i + 1:]
            break
        if sub is None or sub not in BLOCKED[exe]:
            continue
        if exe == "gh":
            # Only `gh pr merge` matters; `gh pr diff`/`list`/`view` are how you review.
            if not rest or rest[0] != "merge":
                continue
            return "gh pr merge"
        if any(flag in rest for flag in ALLOW_SUFFIXES.get(sub, ())):
            continue
        # `git apply`/`am` only matter when they are fed something from outside.
        if sub in ("apply", "am") and not rest:
            continue
        return f"git {sub}"
    return None


def main() -> int:
    try:
        event = json.load(sys.stdin)
    except (ValueError, OSError):
        return 0                         # not a tool call this hook understands: do not interfere
    if not isinstance(event, dict):
        return 0
    if event.get("tool_name") not in ("Bash", "PowerShell"):
        return 0
    command = (event.get("tool_input") or {}).get("command") or ""
    if not isinstance(command, str) or not command.strip():
        return 0

    what = offending(command)
    if what is None:
        return 0

    print(
        f"BLOCKED by scripts/dev/merge-guard.py: `{what}` may not be run directly.\n"
        "\n"
        "Merging a public pull request has to go through the security gate, because a merge is\n"
        "followed by a build, and a build runs the pull request's build.rs, its procedural\n"
        "macros and anything under scripts/ as the user. Use:\n"
        "\n"
        f"    python scripts/dev/merge-pr.py <NUMBER> --repo ivall/xcl\n"
        "\n"
        "It runs scripts/dev/pr-gate.py first (no checkout, nothing executed), requires a\n"
        "receipt for the pull request's exact head commit, verifies that what it fetched is\n"
        "what was reviewed, and only then merges with --no-ff.\n"
        "\n"
        "Recovery and read-only forms are not blocked: `git merge --abort`, `--continue`,\n"
        "`git merge-base`, `git pull --ff-only`, `gh pr diff`.\n"
        "\n"
        "Do not work around this by other means. If the user wants this merge without the\n"
        "gate, they have to say so explicitly.",
        file=sys.stderr)
    return 2                             # exit 2 blocks the tool call


if __name__ == "__main__":
    sys.exit(main())
