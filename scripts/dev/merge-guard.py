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
import subprocess
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

ROOT = os.environ.get("CLAUDE_PROJECT_DIR") or os.path.dirname(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# This fork's own remote branches. A commit already reachable from one of these arrived through
# the hourly sync workflow, not from a stranger's branch, so merging it is routine work and must
# not be blocked - the gate exists to stop us *building* someone else's code, and 154 synced
# upstream commits cannot be reviewed one pull request at a time.
OWN_REFS = ("origin/dev", "origin/main")
# Never treated as our own, whatever it resolves to: FETCH_HEAD is exactly how a foreign pull
# request arrives, and a URL fetches from outside.
FOREIGN = re.compile(r"^(FETCH_HEAD|MERGE_HEAD)$|://|^git@", re.I)
# Flags whose next token is a value, not a ref.
VALUE_FLAGS = {"-m", "-F", "--message", "--file", "-X", "--strategy", "--strategy-option",
               "-S", "--gpg-sign", "--into-name", "-s"}


def git_ok(*args: str) -> bool:
    try:
        return subprocess.run(["git", "-C", ROOT, *args],
                              capture_output=True, timeout=15).returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False


def git_out(*args: str) -> str:
    try:
        r = subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True,
                           errors="replace", timeout=15)
        return r.stdout.strip() if r.returncode == 0 else ""
    except (OSError, subprocess.SubprocessError):
        return ""


def is_ours(ref: str) -> bool:
    """Whether `ref` names a commit already reachable from this fork's own remote branches."""
    if not ref or FOREIGN.search(ref):
        return False
    return any(git_ok("merge-base", "--is-ancestor", ref, own) for own in OWN_REFS)


def refs_of(sub: str, rest: list) -> "list[str]":
    """The refs a merge/pull/cherry-pick names, with flags and their values removed.
    `git pull origin dev` is folded into `origin/dev`."""
    refs, i = [], 0
    while i < len(rest):
        t = rest[i]
        if t in VALUE_FLAGS:
            i += 2
            continue
        if t.startswith("-"):
            i += 1
            continue
        refs.append(t)
        i += 1
    if sub == "pull" and len(refs) == 2 and "/" not in refs[0]:
        return [f"{refs[0]}/{refs[1]}"]      # `git pull origin dev`
    return refs


def own_work(sub: str, rest: list) -> bool:
    """Whether this invocation only moves commits that are already in our own fork."""
    refs = refs_of(sub, rest)
    if not refs:
        # `git pull` with no ref follows the branch's upstream.
        if sub != "pull":
            return False
        upstream = git_out("rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}")
        return upstream in OWN_REFS
    return all(is_ours(r) for r in refs)


HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def strip_heredocs(command: str) -> str:
    """Remove here-document bodies. A heredoc body is data, not commands: a commit message that
    quotes `git merge FETCH_HEAD` must not read as an attempt to run it. (This guard blocked its
    own commit that way once.)"""
    out, pos = [], 0
    while True:
        m = HEREDOC.search(command, pos)
        if not m:
            out.append(command[pos:])
            return "".join(out)
        marker = m.group(2)
        line_end = command.find("\n", m.end())
        if line_end == -1:
            out.append(command[pos:m.end()])
            return "".join(out)
        out.append(command[pos:line_end + 1])
        # Body runs to a line that is exactly the marker (<<- allows leading tabs/spaces).
        body = command[line_end + 1:]
        end = re.search(rf"(?m)^[ \t]*{re.escape(marker)}[ \t]*$", body)
        pos = line_end + 1 + (end.end() if end else len(body))


def segments(command: str) -> list:
    """The command split into separate invocations, so `cd x && git merge y` is still seen.

    Tokenised with shlex *before* splitting on separators, so an `&&` or `;` inside a quoted
    string is data rather than a boundary - `git commit -m "see git merge"` is not a merge."""
    out = []
    # Newline first: shlex treats it as plain whitespace, so without this a command on the line
    # after a heredoc would be folded into the one before it and never examined.
    for line in strip_heredocs(command).splitlines():
        if not line.strip():
            continue
        try:
            tokens = shlex.split(line, comments=True)
        except ValueError:                  # unbalanced quotes: fall back to the blunt split
            out += [s.split() for s in SPLIT.split(line) if s.strip()]
            continue
        current = []
        for t in tokens:
            if t in ("&&", "||", ";", "|", "&"):
                if current:
                    out.append(current)
                current = []
            else:
                current.append(t)
        if current:
            out.append(current)
    return out


def offending(command: str) -> "str | None":
    """The blocked invocation in this command, or None."""
    if SANCTIONED.search(command):
        return None                      # the wrapper is the sanctioned path; it runs the gate
    for tokens in segments(command):
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
        # Syncing our own fork is routine work, not an un-gated merge of someone else's code.
        if sub in ("merge", "pull", "cherry-pick") and own_work(sub, rest):
            continue
        return f"git {sub}"
    return None


def main() -> int:
    raw = sys.stdin.read()
    try:
        event = json.loads(raw)
    except ValueError:
        # Unparseable input must not mean "allow": scan the raw text for the same patterns, so
        # a malformed event cannot become a way through.
        event = None
    if isinstance(event, dict):
        if event.get("tool_name") not in ("Bash", "PowerShell"):
            return 0
        command = (event.get("tool_input") or {}).get("command") or ""
        if not isinstance(command, str) or not command.strip():
            return 0
    else:
        command = raw

    what = offending(command)
    if what is None and event is None:
        # Unparseable event: the structured parse above cannot be trusted, so also look for a
        # blocked invocation anywhere in the raw text rather than treating "I could not read
        # this" as permission.
        loose = re.search(r"\bgit\b[^\n;|&]*\b(merge|cherry-pick|pull|am|apply|revert)\b"
                          r"|\bgh\b[^\n;|&]*\bpr\b[^\n;|&]*\bmerge\b", command)
        if loose and not SANCTIONED.search(command):
            what = f"git {loose.group(1) or 'pr merge'}"
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
    # Fail closed. Claude Code reads a non-zero-but-not-2 exit as "allow", so an unhandled
    # exception in this guard would silently permit exactly what it exists to stop - which is
    # how a missing `import subprocess` once made every merge allowed. Any internal error
    # blocks instead, loudly.
    try:
        sys.exit(main())
    except SystemExit:
        raise
    except BaseException as error:   # noqa: BLE001 - deliberate catch-all
        import traceback
        print(f"BLOCKED by scripts/dev/merge-guard.py: the guard itself failed "
              f"({type(error).__name__}: {error}), so it cannot say whether this command is "
              "safe. Blocking rather than guessing.\n\n"
              + traceback.format_exc(), file=sys.stderr)
        sys.exit(2)
